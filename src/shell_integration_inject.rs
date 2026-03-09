//! Auto-inject OSC 133 shell integration into new shells.
//!
//! Writes shell integration scripts to ~/.config/pmux/ and provides
//! environment setup for auto-sourcing when shells start inside pmux.
//!
//! This is similar to how iTerm2, VS Code terminal, Warp, and kitty
//! auto-inject shell integration for OSC 133 support.

use std::fs;
use std::path::PathBuf;

/// zsh shell integration script content.
/// Uses add-zsh-hook to avoid overwriting user's existing precmd/preexec.
const ZSH_INTEGRATION: &str = r#"# pmux shell integration — OSC 133 markers for agent status detection.
# Auto-sourced by pmux; do not edit. Re-generated on each launch.
#
# Markers: A=PromptStart, B=PromptEnd, C=PreExec, D=PostExec(exit_code)

# Guard: only load once per shell session
[[ -n "$_PMUX_OSC133_LOADED" ]] && return
_PMUX_OSC133_LOADED=1

autoload -Uz add-zsh-hook

_pmux_osc133_precmd() {
  local ret=$?
  # D: previous command finished with exit code
  printf '\033]133;D;%d\007' "$ret"
  # A: prompt is about to be drawn
  printf '\033]133;A\007'
}

_pmux_osc133_preexec() {
  # C: command is about to execute
  printf '\033]133;C\007'
}

add-zsh-hook precmd _pmux_osc133_precmd
add-zsh-hook preexec _pmux_osc133_preexec

# Emit initial A marker for the very first prompt
printf '\033]133;A\007'
"#;

/// bash shell integration script content.
const BASH_INTEGRATION: &str = r#"# pmux shell integration — OSC 133 markers for agent status detection.
# Auto-sourced by pmux; do not edit. Re-generated on each launch.

# Guard: only load once
[[ -n "$_PMUX_OSC133_LOADED" ]] && return
_PMUX_OSC133_LOADED=1

_pmux_osc133_prompt() {
  local ret=$?
  printf '\033]133;D;%d\007' "$ret"
  printf '\033]133;A\007'
}

if [[ -z "$PROMPT_COMMAND" ]]; then
  PROMPT_COMMAND="_pmux_osc133_prompt"
else
  PROMPT_COMMAND="_pmux_osc133_prompt;${PROMPT_COMMAND}"
fi

trap 'printf "\033]133;C\007"' DEBUG

printf '\033]133;A\007'
"#;

/// fish shell integration script content.
const FISH_INTEGRATION: &str = r#"# pmux shell integration — OSC 133 markers for agent status detection.
# Auto-sourced by pmux; do not edit. Re-generated on each launch.

if set -q _PMUX_OSC133_LOADED
  exit
end
set -g _PMUX_OSC133_LOADED 1

function _pmux_osc133_prompt --on-event fish_prompt
  printf '\033]133;D;%d\007' $status
  printf '\033]133;A\007'
end

function _pmux_osc133_preexec --on-event fish_preexec
  printf '\033]133;C\007'
end

printf '\033]133;A\007'
"#;

/// Get the pmux config directory (~/.config/pmux/).
fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("pmux"))
}

/// Write all shell integration scripts to ~/.config/pmux/.
/// Called once on app startup. Overwrites existing scripts (they're auto-generated).
pub fn ensure_shell_integration_scripts() {
    let Some(dir) = config_dir() else { return };
    let _ = fs::create_dir_all(&dir);

    let scripts = [
        ("shell-integration.zsh", ZSH_INTEGRATION),
        ("shell-integration.bash", BASH_INTEGRATION),
        ("shell-integration.fish", FISH_INTEGRATION),
    ];

    for (name, content) in scripts {
        let path = dir.join(name);
        let _ = fs::write(&path, content);
    }
}

/// Get the path to the shell integration script for a given shell.
/// Returns None if the config dir can't be determined.
pub fn integration_script_path(shell: &str) -> Option<PathBuf> {
    let dir = config_dir()?;
    let ext = match shell {
        "zsh" => "zsh",
        "bash" => "bash",
        "fish" => "fish",
        _ => return None,
    };
    Some(dir.join(format!("shell-integration.{}", ext)))
}

/// Build the source command to inject shell integration for a given shell.
/// Returns a command string that can be sent via tmux send-keys.
pub fn source_command(shell: &str) -> Option<String> {
    let path = integration_script_path(shell)?;
    let path_str = path.to_str()?;
    match shell {
        "zsh" | "bash" => Some(format!("source '{}' 2>/dev/null", path_str)),
        "fish" => Some(format!("source '{}' 2>/dev/null", path_str)),
        _ => None,
    }
}

/// Detect the default shell name (e.g., "zsh", "bash", "fish").
pub fn detect_shell() -> String {
    // Check SHELL env var
    if let Ok(shell) = std::env::var("SHELL") {
        if let Some(name) = shell.rsplit('/').next() {
            return name.to_string();
        }
    }
    "zsh".to_string() // fallback
}
