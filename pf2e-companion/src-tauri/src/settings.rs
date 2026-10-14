//! App settings — the 3-key `settings.json` in app-data.
//!
//! Hand-rolled on purpose (no tauri-plugin-store): the project hand-rolls
//! every persistence layer, and a vault path + two booleans don't justify a
//! new dep + capability + lockfile churn. Atomic tmp+rename write follows
//! the `SessionService::save_now` pattern.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    /// None = default vault (`<app_data>/vault`).
    #[serde(default)]
    pub vault_path: Option<PathBuf>,
    #[serde(default)]
    pub wizard_completed: bool,
    /// Vault root the previous launch ran under — change detection for the
    /// derived-index reset in `lib.rs` setup.
    #[serde(default)]
    pub last_run_vault: Option<PathBuf>,
}

/// `<app_data>/settings.json`
pub fn settings_path(app_data: &Path) -> PathBuf {
    app_data.join("settings.json")
}

/// Missing or corrupt file → defaults (corrupt logs a warning). Infallible
/// on purpose — this runs before the window opens; a panic here kills the
/// app at startup.
pub fn load(app_data: &Path) -> AppSettings {
    let path = settings_path(app_data);
    match std::fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "corrupt settings.json, using defaults");
                AppSettings::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => AppSettings::default(),
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "unreadable settings.json, using defaults");
            AppSettings::default()
        }
    }
}

/// Atomic `.tmp` + rename; parent dirs created.
pub fn save(app_data: &Path, s: &AppSettings) -> Result<()> {
    let path = settings_path(app_data);
    std::fs::create_dir_all(app_data)
        .with_context(|| format!("create {}", app_data.display()))?;
    let tmp = path.with_extension("json.tmp");
    let raw = serde_json::to_string_pretty(s).context("serialize settings")?;
    std::fs::write(&tmp, raw).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

/// The vault root this launch should use.
pub fn effective_vault(app_data: &Path, s: &AppSettings) -> PathBuf {
    s.vault_path
        .clone()
        .unwrap_or_else(|| app_data.join("vault"))
}

/// Load + existing-install guard: if the wizard hasn't been completed yet
/// but the default vault already has campaigns, the install predates the
/// wizard — mark it completed and persist. A user who only ever browsed
/// bundled content (empty vault) is effectively first-run for vault
/// purposes, so showing them the wizard is correct, not a regression.
pub fn load_or_migrate(app_data: &Path) -> AppSettings {
    let mut s = load(app_data);
    if !s.wizard_completed && default_vault_has_campaigns(app_data) {
        s.wizard_completed = true;
        if let Err(e) = save(app_data, &s) {
            tracing::warn!(error = %e, "failed to persist wizard migration");
        }
    }
    s
}

fn default_vault_has_campaigns(app_data: &Path) -> bool {
    std::fs::read_dir(app_data.join("vault").join("campaigns"))
        .map(|entries| entries.filter_map(|e| e.ok()).count() > 0)
        .unwrap_or(false)
}

/// Validate a user-supplied vault path. Rejects empty / relative / root;
/// creates the directory so a brand-new folder works; returns the
/// canonicalized path. Errors are user-facing strings.
pub fn validate_vault_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Vault path is empty.".into());
    }
    let path = PathBuf::from(trimmed);
    if path.is_relative() {
        return Err("Vault path must be absolute (a full path starting from the filesystem root).".into());
    }
    if path == Path::new("/") {
        return Err("The filesystem root can't be a vault.".into());
    }
    // Well-known system roots are never a vault (a watcher ingesting /etc
    // is not worldbuilding). Guard BEFORE create_dir_all so a missing
    // subdir of one is never created on our behalf. Component-wise
    // starts_with: "/etc2" is fine, "/etc/foo" is not.
    const SYSTEM_DIRS: [&str; 6] = ["/etc", "/usr", "/bin", "/sbin", "/var", "/boot"];
    if SYSTEM_DIRS.iter().any(|d| path.starts_with(d))
        || path.starts_with("C:\\Windows")
        || path.starts_with("C:\\Program Files")
    {
        return Err("That's a system folder — pick a folder for your campaign content.".into());
    }
    std::fs::create_dir_all(&path)
        .map_err(|e| format!("Can't create vault folder {}: {e}", path.display()))?;
    path.canonicalize()
        .map_err(|e| format!("Can't resolve vault folder {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn defaults_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let s = load(dir.path());
        assert_eq!(s, AppSettings::default());
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = TempDir::new().unwrap();
        std::fs::write(settings_path(dir.path()), "{not json").unwrap();
        let s = load(dir.path());
        assert_eq!(s, AppSettings::default());
    }

    #[test]
    fn save_load_round_trip_tolerates_unknown_keys() {
        let dir = TempDir::new().unwrap();
        let mut s = AppSettings::default();
        s.vault_path = Some(PathBuf::from("/tmp/custom-vault"));
        s.wizard_completed = true;
        s.last_run_vault = Some(PathBuf::from("/tmp/custom-vault"));
        save(dir.path(), &s).unwrap();
        assert!(settings_path(dir.path()).exists());

        // Forward compat: unknown keys from a newer version are ignored.
        let raw = std::fs::read_to_string(settings_path(dir.path())).unwrap();
        let patched = raw.replace(
            "{\n",
            "{\n  \"future_key\": \"ignored\",\n",
        );
        std::fs::write(settings_path(dir.path()), patched).unwrap();

        let loaded = load(dir.path());
        assert_eq!(loaded, s);
    }

    #[test]
    fn validate_vault_path_accepts_creates_and_canonicalizes() {
        let dir = TempDir::new().unwrap();
        let missing_sub = dir.path().join("vault-a").join("inner");
        let normalized = validate_vault_path(missing_sub.to_str().unwrap()).unwrap();
        assert!(normalized.is_dir(), "missing subdirectory should be created");
        assert_eq!(normalized, missing_sub.canonicalize().unwrap());

        let existing = validate_vault_path(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(existing, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn validate_vault_path_rejects_bad_input() {
        assert!(validate_vault_path("").is_err());
        assert!(validate_vault_path("   ").is_err());
        assert!(validate_vault_path("relative/path").is_err());
        assert!(validate_vault_path("/").is_err());
    }

    #[test]
    fn validate_vault_path_rejects_system_dirs() {
        assert!(validate_vault_path("/etc").is_err());
        assert!(validate_vault_path("/usr/share").is_err());
        assert!(validate_vault_path("C:\\Windows").is_err());
        // Component-wise compare: /etc2 is NOT a system dir.
        let dir = TempDir::new().unwrap();
        let lookalike = dir.path().join("etc2");
        assert!(validate_vault_path(lookalike.to_str().unwrap()).is_ok());
    }
}
