// window_state.rs - Window state persistence for pmux
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Window state structure
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// Window size (width, height) in pixels
    pub size: (u32, u32),
    /// Window position (x, y) in screen coordinates
    pub position: (i32, i32),
    /// Whether window is maximized
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            size: (1200, 800),
            position: (100, 100),
            maximized: false,
        }
    }
}

impl WindowState {
    /// Create new window state with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with specific values
    pub fn with_values(width: u32, height: u32, x: i32, y: i32, maximized: bool) -> Self {
        Self {
            size: (width, height),
            position: (x, y),
            maximized,
        }
    }

    /// Update window size
    pub fn update_size(&mut self, width: u32, height: u32) {
        if !self.maximized {
            self.size = (width, height);
        }
    }

    /// Update window position
    pub fn update_position(&mut self, x: i32, y: i32) {
        if !self.maximized {
            self.position = (x, y);
        }
    }

    /// Set maximized state
    pub fn set_maximized(&mut self, maximized: bool) {
        self.maximized = maximized;
    }

    /// Get window width
    pub fn width(&self) -> u32 {
        self.size.0
    }

    /// Get window height
    pub fn height(&self) -> u32 {
        self.size.1
    }

    /// Get x position
    pub fn x(&self) -> i32 {
        self.position.0
    }

    /// Get y position
    pub fn y(&self) -> i32 {
        self.position.1
    }
}

/// Application state aggregate structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistentAppState {
    /// Window state
    pub window_state: WindowState,
    /// Sidebar width in pixels
    pub sidebar_width: u32,
    /// Active workspace index
    pub active_workspace_index: usize,
    /// Recent workspaces list
    pub recent_workspaces: Vec<PathBuf>,
    /// Last saved timestamp
    pub last_saved: Option<u64>,
}

impl Default for PersistentAppState {
    fn default() -> Self {
        Self {
            window_state: WindowState::default(),
            sidebar_width: 280,
            active_workspace_index: 0,
            recent_workspaces: Vec::new(),
            last_saved: None,
        }
    }
}

impl PersistentAppState {
    /// Create new persistent app state
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a recent workspace (moves to front, deduplicates)
    pub fn add_recent_workspace(&mut self, path: PathBuf) {
        // Remove if already exists
        self.recent_workspaces.retain(|p| p != &path);
        // Add to front
        self.recent_workspaces.insert(0, path);
        // Keep only last 10
        if self.recent_workspaces.len() > 10 {
            self.recent_workspaces.truncate(10);
        }
    }

    /// Remove a recent workspace
    pub fn remove_recent_workspace(&mut self, path: &PathBuf) {
        self.recent_workspaces.retain(|p| p != path);
    }

    /// Clear recent workspaces
    pub fn clear_recent_workspaces(&mut self) {
        self.recent_workspaces.clear();
    }

    /// Update last saved timestamp
    pub fn touch(&mut self) {
        self.last_saved = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
    }

    /// Get config file path
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("pmux").join("state.json"))
    }

    /// Save to config file
    pub fn save(&mut self) -> Result<(), StatePersistenceError> {
        let path = Self::config_path()
            .ok_or(StatePersistenceError::ConfigDirNotFound)?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Update timestamp
        self.touch();

        // Serialize and save
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;

        Ok(())
    }

    /// Load from config file
    pub fn load() -> Result<Self, StatePersistenceError> {
        let path = Self::config_path()
            .ok_or(StatePersistenceError::ConfigDirNotFound)?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let json = std::fs::read_to_string(&path)?;
        let state: Self = serde_json::from_str(&json)?;
        Ok(state)
    }

    /// Check if config file exists
    pub fn exists() -> bool {
        Self::config_path()
            .map(|p| p.exists())
            .unwrap_or(false)
    }
}

/// State persistence errors
#[derive(Debug, thiserror::Error)]
pub enum StatePersistenceError {
    #[error("Config directory not found")]
    ConfigDirNotFound,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Auto-save manager
#[derive(Clone, Debug)]
pub struct AutoSaveManager {
    state: PersistentAppState,
    last_save: Instant,
    interval: Duration,
    dirty: bool,
}

impl AutoSaveManager {
    /// Create new auto-save manager
    pub fn new(interval_secs: u64) -> Self {
        Self {
            state: PersistentAppState::new(),
            last_save: Instant::now(),
            interval: Duration::from_secs(interval_secs),
            dirty: false,
        }
    }

    /// Load existing state or create new
    pub fn load_or_new(interval_secs: u64) -> Self {
        let state = PersistentAppState::load().unwrap_or_default();
        Self {
            state,
            last_save: Instant::now(),
            interval: Duration::from_secs(interval_secs),
            dirty: false,
        }
    }

    /// Mark state as dirty (needs saving)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Update window state and mark dirty
    pub fn update_window(&mut self, window: WindowState) {
        self.state.window_state = window;
        self.mark_dirty();
    }

    /// Update sidebar width and mark dirty
    pub fn update_sidebar_width(&mut self, width: u32) {
        self.state.sidebar_width = width;
        self.mark_dirty();
    }

    /// Update active workspace and mark dirty
    pub fn update_active_workspace(&mut self, index: usize) {
        self.state.active_workspace_index = index;
        self.mark_dirty();
    }

    /// Add recent workspace and mark dirty
    pub fn add_recent_workspace(&mut self, path: PathBuf) {
        self.state.add_recent_workspace(path);
        self.mark_dirty();
    }

    /// Check if should auto-save
    pub fn should_save(&self) -> bool {
        self.dirty && self.last_save.elapsed() >= self.interval
    }

    /// Perform save if needed
    pub fn tick(&mut self) -> Result<bool, StatePersistenceError> {
        if self.should_save() {
            self.save_now()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Force save now
    pub fn save_now(&mut self) -> Result<(), StatePersistenceError> {
        self.state.save()?;
        self.last_save = Instant::now();
        self.dirty = false;
        Ok(())
    }

    /// Get reference to state
    pub fn state(&self) -> &PersistentAppState {
        &self.state
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut PersistentAppState {
        &mut self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_state_default() {
        let state = WindowState::default();
        assert_eq!(state.size, (1200, 800));
        assert_eq!(state.position, (100, 100));
        assert!(!state.maximized);
    }

    #[test]
    fn test_window_state_with_values() {
        let state = WindowState::with_values(1920, 1080, 50, 50, true);
        assert_eq!(state.width(), 1920);
        assert_eq!(state.height(), 1080);
        assert_eq!(state.x(), 50);
        assert_eq!(state.y(), 50);
        assert!(state.maximized);
    }

    #[test]
    fn test_window_state_update() {
        let mut state = WindowState::new();
        state.update_size(800, 600);
        assert_eq!(state.size, (800, 600));

        state.update_position(200, 200);
        assert_eq!(state.position, (200, 200));
    }

    #[test]
    fn test_window_state_no_update_when_maximized() {
        let mut state = WindowState::with_values(100, 100, 0, 0, true);
        state.update_size(200, 200);
        assert_eq!(state.size, (100, 100)); // Should not change
    }

    #[test]
    fn test_persistent_app_state_default() {
        let state = PersistentAppState::default();
        assert_eq!(state.sidebar_width, 250);
        assert_eq!(state.active_workspace_index, 0);
        assert!(state.recent_workspaces.is_empty());
        assert!(state.last_saved.is_none());
    }

    #[test]
    fn test_add_recent_workspace() {
        let mut state = PersistentAppState::new();
        state.add_recent_workspace(PathBuf::from("/path/one"));
        state.add_recent_workspace(PathBuf::from("/path/two"));

        assert_eq!(state.recent_workspaces.len(), 2);
        assert_eq!(state.recent_workspaces[0], PathBuf::from("/path/two"));
    }

    #[test]
    fn test_add_recent_workspace_dedup() {
        let mut state = PersistentAppState::new();
        state.add_recent_workspace(PathBuf::from("/path/one"));
        state.add_recent_workspace(PathBuf::from("/path/two"));
        state.add_recent_workspace(PathBuf::from("/path/one")); // Duplicate

        assert_eq!(state.recent_workspaces.len(), 2);
        assert_eq!(state.recent_workspaces[0], PathBuf::from("/path/one"));
    }

    #[test]
    fn test_add_recent_workspace_limit() {
        let mut state = PersistentAppState::new();
        for i in 0..15 {
            state.add_recent_workspace(PathBuf::from(format!("/path/{}", i)));
        }

        assert_eq!(state.recent_workspaces.len(), 10);
    }

    #[test]
    fn test_remove_recent_workspace() {
        let mut state = PersistentAppState::new();
        state.add_recent_workspace(PathBuf::from("/path/one"));
        state.add_recent_workspace(PathBuf::from("/path/two"));

        state.remove_recent_workspace(&PathBuf::from("/path/one"));
        assert_eq!(state.recent_workspaces.len(), 1);
        assert_eq!(state.recent_workspaces[0], PathBuf::from("/path/two"));
    }

    #[test]
    fn test_touch_updates_timestamp() {
        let mut state = PersistentAppState::new();
        assert!(state.last_saved.is_none());

        state.touch();
        assert!(state.last_saved.is_some());
    }

    #[test]
    fn test_auto_save_manager() {
        let mut manager = AutoSaveManager::new(60);
        assert!(!manager.should_save());

        manager.mark_dirty();
        // Won't save immediately because interval hasn't passed
        assert!(manager.should_save()); // But should_save returns true when dirty + interval passed

        // Since we can't wait in tests, let's just verify the logic
        manager.dirty = false;
        assert!(!manager.should_save());
    }

    #[test]
    fn test_auto_save_manager_updates() {
        let mut manager = AutoSaveManager::new(60);

        manager.update_sidebar_width(300);
        assert!(manager.dirty);
        assert_eq!(manager.state.sidebar_width, 300);

        manager.update_active_workspace(2);
        assert_eq!(manager.state.active_workspace_index, 2);
    }
}
