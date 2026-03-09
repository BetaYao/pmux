// updater.rs - Auto-update checking, downloading, and installation
use serde::Deserialize;
use thiserror::Error;
use std::path::{Path, PathBuf};

const GITHUB_API_URL: &str = "https://api.github.com/repos/zhoujinliang/pmux/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const USER_AGENT: &str = concat!("pmux/", env!("CARGO_PKG_VERSION"));

#[derive(Error, Debug)]
pub enum UpdateError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("No matching asset found for this platform")]
    NoAsset,
    #[error("Version parse error: {0}")]
    VersionParse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Could not determine app bundle path")]
    AppPathNotFound,
    #[error("Download failed: {0}")]
    Download(String),
    #[error("Extraction failed: {0}")]
    Extraction(String),
}

/// Semantic version for comparison
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    /// Parse from tag string like "v0.1.2" or "0.1.2"
    pub fn parse(tag: &str) -> Option<Self> {
        let s = tag.strip_prefix('v').unwrap_or(tag);
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(Self {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }

    pub fn display(&self) -> String {
        format!("v{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Subset of GitHub Release API response
#[derive(Debug, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub html_url: String,
    pub assets: Vec<GitHubAsset>,
    pub body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

/// Available update information
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current_version: SemVer,
    pub latest_version: SemVer,
    pub download_url: String,
    pub release_url: String,
    pub asset_size: u64,
}

pub enum UpdateCheckResult {
    UpdateAvailable(UpdateInfo),
    UpToDate,
    Skipped,
}

/// Check GitHub for a newer release. Runs blocking HTTP — call from background thread.
pub fn check_for_update(skipped_version: Option<&str>) -> Result<UpdateCheckResult, UpdateError> {
    let current = SemVer::parse(CURRENT_VERSION)
        .ok_or_else(|| UpdateError::VersionParse(CURRENT_VERSION.to_string()))?;

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let release: GitHubRelease = client
        .get(GITHUB_API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()?
        .json()?;

    let latest = SemVer::parse(&release.tag_name)
        .ok_or_else(|| UpdateError::VersionParse(release.tag_name.clone()))?;

    if latest <= current {
        return Ok(UpdateCheckResult::UpToDate);
    }

    if let Some(skipped) = skipped_version {
        if let Some(skipped_ver) = SemVer::parse(skipped) {
            if skipped_ver == latest {
                return Ok(UpdateCheckResult::Skipped);
            }
        }
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.contains("macos-arm64") && a.name.ends_with(".tar.gz"))
        .ok_or(UpdateError::NoAsset)?;

    Ok(UpdateCheckResult::UpdateAvailable(UpdateInfo {
        current_version: current,
        latest_version: latest,
        download_url: asset.browser_download_url.clone(),
        release_url: release.html_url,
        asset_size: asset.size,
    }))
}

/// Download the update tar.gz, extract, and replace the current .app bundle.
/// Returns the path to the updated .app for relaunch.
pub fn download_and_install(info: &UpdateInfo) -> Result<PathBuf, UpdateError> {
    let current_exe = std::env::current_exe()?;
    let app_bundle = find_app_bundle(&current_exe).ok_or(UpdateError::AppPathNotFound)?;

    // Download to temp directory
    let tmp_dir = std::env::temp_dir().join(format!("pmux-update-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir)?;

    let tarball_path = tmp_dir.join("pmux-update.tar.gz");

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let mut response = client.get(&info.download_url).send()?;
    if !response.status().is_success() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(UpdateError::Download(format!("HTTP {}", response.status())));
    }
    let mut file = std::fs::File::create(&tarball_path)?;
    std::io::copy(&mut response, &mut file)?;
    drop(file);

    // Extract
    let extract_dir = tmp_dir.join("extracted");
    std::fs::create_dir_all(&extract_dir)?;
    let status = std::process::Command::new("tar")
        .args(["xzf"])
        .arg(&tarball_path)
        .current_dir(&extract_dir)
        .status()?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(UpdateError::Extraction("tar extraction failed".into()));
    }

    let new_app = extract_dir.join("pmux.app");
    if !new_app.exists() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(UpdateError::Extraction(
            "pmux.app not found in archive".into(),
        ));
    }

    // Replace: backup old → copy new → cleanup
    let backup_path = app_bundle.with_extension("app.old");
    if backup_path.exists() {
        let _ = std::fs::remove_dir_all(&backup_path);
    }
    std::fs::rename(&app_bundle, &backup_path)?;

    // cp -R preserves code signatures, symlinks, extended attributes
    let cp_status = std::process::Command::new("cp")
        .args(["-R"])
        .arg(&new_app)
        .arg(&app_bundle)
        .status()?;

    if !cp_status.success() {
        // Restore backup on failure
        let _ = std::fs::rename(&backup_path, &app_bundle);
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(UpdateError::Extraction(
            "Failed to copy new app bundle".into(),
        ));
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&backup_path);
    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(app_bundle)
}

/// Find the .app bundle root from an executable path.
/// E.g. /Applications/pmux.app/Contents/MacOS/pmux -> /Applications/pmux.app
pub fn find_app_bundle(exe_path: &Path) -> Option<PathBuf> {
    let mut current = exe_path.to_path_buf();
    loop {
        if current
            .extension()
            .map_or(false, |e| e == "app")
        {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Relaunch pmux after update. Spawns a new instance and exits current process.
pub fn relaunch(app_path: &Path) {
    let _ = std::process::Command::new("open")
        .args(["-n"])
        .arg(app_path)
        .spawn();
    std::process::exit(0);
}

/// Return the current version string from Cargo.toml
pub fn current_version() -> &'static str {
    CURRENT_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_parse_with_v_prefix() {
        let v = SemVer::parse("v0.1.2").unwrap();
        assert_eq!(
            v,
            SemVer {
                major: 0,
                minor: 1,
                patch: 2
            }
        );
    }

    #[test]
    fn test_semver_parse_without_prefix() {
        let v = SemVer::parse("1.2.3").unwrap();
        assert_eq!(
            v,
            SemVer {
                major: 1,
                minor: 2,
                patch: 3
            }
        );
    }

    #[test]
    fn test_semver_parse_invalid() {
        assert!(SemVer::parse("not-a-version").is_none());
        assert!(SemVer::parse("v1.2").is_none());
        assert!(SemVer::parse("").is_none());
    }

    #[test]
    fn test_semver_comparison() {
        let v1 = SemVer::parse("v0.1.0").unwrap();
        let v2 = SemVer::parse("v0.1.1").unwrap();
        let v3 = SemVer::parse("v0.2.0").unwrap();
        let v4 = SemVer::parse("v1.0.0").unwrap();
        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
        assert_eq!(v1, SemVer::parse("v0.1.0").unwrap());
    }

    #[test]
    fn test_semver_display() {
        let v = SemVer {
            major: 0,
            minor: 2,
            patch: 1,
        };
        assert_eq!(v.display(), "v0.2.1");
    }

    #[test]
    fn test_find_app_bundle_applications() {
        let exe = Path::new("/Applications/pmux.app/Contents/MacOS/pmux");
        assert_eq!(
            find_app_bundle(exe),
            Some(PathBuf::from("/Applications/pmux.app"))
        );
    }

    #[test]
    fn test_find_app_bundle_target() {
        let exe = Path::new("/Users/me/workspace/pmux/target/release/bundle/osx/pmux.app/Contents/MacOS/pmux");
        assert_eq!(
            find_app_bundle(exe),
            Some(PathBuf::from(
                "/Users/me/workspace/pmux/target/release/bundle/osx/pmux.app"
            ))
        );
    }

    #[test]
    fn test_find_app_bundle_no_app() {
        let exe = Path::new("/usr/local/bin/pmux");
        assert!(find_app_bundle(exe).is_none());
    }

    #[test]
    fn test_current_version_not_empty() {
        assert!(!current_version().is_empty());
        assert!(SemVer::parse(current_version()).is_some());
    }
}
