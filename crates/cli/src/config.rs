//! CLI-side persistent config for `aria-memo` (mirrors the trimmed-down
//! `router-cli.yml` portion of aria-router's config crate).
//!
//! Unlike router, memo has no gateway recipe / keys / users, so the only
//! persisted setting today is `upgrade_url` consumed by `aria-memo upgrade`.
//! The file lives at `~/.ariacompute/memo-cli.yml` (distinct from router's
//! `router-cli.yml`) and the home dir is overridable via `ARIA_COMPUTE_HOME`.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;

/// Default Releases org root (GitHub). This is the owner root
/// (`https://github.com/ariacompute`), not the repo URL — `upgrade` appends
/// `/memo/releases` when building the API path, so it must NOT include `/memo`.
pub const DEFAULT_UPGRADE_URL_COM: &str = "https://github.com/ariacompute";
/// Default Releases org root for `.cn` (Gitee). Same owner-root convention.
pub const DEFAULT_UPGRADE_URL_CN: &str = "https://gitee.com/ariacompute";

/// `$HOME/.ariacompute` (overridable via `ARIA_COMPUTE_HOME`). Shares the
/// directory with aria-router's config but uses a distinct filename.
pub fn memo_home() -> io::Result<PathBuf> {
    let override_home = std::env::var("ARIA_COMPUTE_HOME").ok();
    resolve_memo_home(override_home.as_deref(), dirs::home_dir())
}

/// Pure home resolution: a non-empty `ARIA_COMPUTE_HOME` wins, otherwise
/// `$HOME/.ariacompute`; an unresolvable home directory is an explicit error.
fn resolve_memo_home(
    override_home: Option<&str>,
    system_home: Option<PathBuf>,
) -> io::Result<PathBuf> {
    if let Some(dir) = override_home {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let home = system_home.ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "could not resolve home directory")
    })?;
    Ok(home.join(".ariacompute"))
}

/// Path to the persistent CLI config: `~/.ariacompute/memo-cli.yml`.
pub fn default_cli_config_path() -> io::Result<PathBuf> {
    Ok(memo_home()?.join("memo-cli.yml"))
}

/// Ensure `~/.ariacompute` exists.
pub fn ensure_memo_home() -> io::Result<PathBuf> {
    let home = memo_home()?;
    std::fs::create_dir_all(&home).map_err(io::Error::other)?;
    Ok(home)
}

/// Pick a default `upgrade_url`. Memo defaults to GitHub; `setup` lets the
/// user choose GitHub/Gitee, and the `upgrade` command's `--upgrade-url` /
/// `ARIA_MEMO_UPGRADE_URL` / `cn` site can still switch to Gitee.
pub fn default_upgrade_url() -> &'static str {
    DEFAULT_UPGRADE_URL_COM
}

/// Pick a default `upgrade_url` by site (`com` | `cn`), mirroring router's
/// `default_upgrade_url_for_site`.
pub fn default_upgrade_url_for_site(site: &str) -> &'static str {
    if site.trim().eq_ignore_ascii_case("cn") {
        DEFAULT_UPGRADE_URL_CN
    } else {
        DEFAULT_UPGRADE_URL_COM
    }
}

/// Sidecar CLI config (separate from any future memo recipe).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoCliConfig {
    #[serde(default)]
    pub upgrade_url: String,
}

/// Load the CLI config. Missing file → default (empty `upgrade_url`).
pub fn load_cli_config() -> io::Result<MemoCliConfig> {
    let path = default_cli_config_path()?;
    if !path.exists() {
        return Ok(MemoCliConfig::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(io::Error::other)?;
    serde_yaml::from_str(&raw).map_err(io::Error::other)
}

/// Persist the CLI config (creates `~/.ariacompute`).
pub fn save_cli_config(cfg: &MemoCliConfig) -> io::Result<PathBuf> {
    ensure_memo_home()?;
    let path = default_cli_config_path()?;
    let raw = serde_yaml::to_string(cfg).map_err(io::Error::other)?;
    std::fs::write(&path, raw).map_err(io::Error::other)?;
    Ok(path)
}

/// Remove the CLI config file if present. Returns its path when deleted.
pub fn clear_cli_config() -> io::Result<Option<PathBuf>> {
    let path = default_cli_config_path()?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(io::Error::other)?;
        return Ok(Some(path));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_empty_upgrade_url() {
        let cfg = MemoCliConfig::default();
        assert!(cfg.upgrade_url.is_empty());
    }

    #[test]
    fn yaml_roundtrip() {
        let cfg = MemoCliConfig {
            upgrade_url: "https://gitee.com/ariacompute".into(),
        };
        let raw = serde_yaml::to_string(&cfg).unwrap();
        assert!(raw.contains("upgrade_url: https://gitee.com/ariacompute"));
        let back: MemoCliConfig = serde_yaml::from_str(&raw).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn default_upgrade_url_is_github() {
        assert_eq!(default_upgrade_url(), DEFAULT_UPGRADE_URL_COM);
    }

    use crate::test_support::{env_lock, temp_dir};

    #[test]
    fn resolve_memo_home_is_pure_and_total() {
        // Explicit override wins outright.
        assert_eq!(
            resolve_memo_home(Some("/tmp/override"), Some(PathBuf::from("/home/u"))).unwrap(),
            PathBuf::from("/tmp/override")
        );
        // Empty override falls through to the system home.
        assert_eq!(
            resolve_memo_home(Some(""), Some(PathBuf::from("/home/u"))).unwrap(),
            PathBuf::from("/home/u/.ariacompute")
        );
        assert_eq!(
            resolve_memo_home(None, Some(PathBuf::from("/home/u"))).unwrap(),
            PathBuf::from("/home/u/.ariacompute")
        );
        // No home at all is an explicit error (never a silent relative path).
        let err = resolve_memo_home(None, None).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn memo_home_env_override() {
        let _g = env_lock();
        let dir = temp_dir("config-home");
        std::env::set_var("ARIA_COMPUTE_HOME", &dir);
        assert_eq!(memo_home().unwrap(), dir);
        assert_eq!(default_cli_config_path().unwrap(), dir.join("memo-cli.yml"));
        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_load_clear_cli_config_roundtrip() {
        let _g = env_lock();
        let dir = temp_dir("config-roundtrip");
        std::env::set_var("ARIA_COMPUTE_HOME", &dir);
        let cfg = MemoCliConfig {
            upgrade_url: "https://gitee.com/ariacompute".into(),
        };
        let path = save_cli_config(&cfg).unwrap();
        assert!(path.exists());
        assert_eq!(load_cli_config().unwrap(), cfg);
        // clear removes the file.
        assert_eq!(clear_cli_config().unwrap(), Some(path.clone()));
        assert!(!path.exists());
        // A missing file loads the default (empty upgrade_url) and clears to None.
        assert_eq!(load_cli_config().unwrap(), MemoCliConfig::default());
        assert_eq!(clear_cli_config().unwrap(), None);
        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_and_clear_report_filesystem_errors() {
        let _g = env_lock();
        let dir = temp_dir("config-read-err");
        std::env::set_var("ARIA_COMPUTE_HOME", &dir);
        let path = default_cli_config_path().unwrap();
        // A directory in place of the config file: reading fails...
        std::fs::create_dir_all(&path).unwrap();
        assert!(load_cli_config().is_err());
        // ...and removing it as a file fails too (never silently reported as cleared).
        std::fs::write(path.join("inner"), b"x").unwrap();
        assert!(clear_cli_config().is_err());
        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_reports_write_failure() {
        let _g = env_lock();
        let dir = temp_dir("config-write-err");
        std::env::set_var("ARIA_COMPUTE_HOME", &dir);
        // Occupying the config path with a non-empty directory makes the write fail.
        let path = default_cli_config_path().unwrap();
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("inner"), b"x").unwrap();
        assert!(save_cli_config(&MemoCliConfig::default()).is_err());
        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_memo_home_reports_create_failure() {
        let _g = env_lock();
        let dir = temp_dir("config-home-err");
        let blocking = dir.join("blocking-file");
        std::fs::write(&blocking, b"x").unwrap();
        // The override points at a regular file, so the home dir cannot be created.
        std::env::set_var("ARIA_COMPUTE_HOME", &blocking);
        assert!(ensure_memo_home().is_err());
        assert!(save_cli_config(&MemoCliConfig::default()).is_err());
        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_upgrade_url_respects_site() {
        assert_eq!(default_upgrade_url_for_site("cn"), DEFAULT_UPGRADE_URL_CN);
        assert_eq!(default_upgrade_url_for_site("CN"), DEFAULT_UPGRADE_URL_CN);
        assert_eq!(default_upgrade_url_for_site("com"), DEFAULT_UPGRADE_URL_COM);
        assert_eq!(
            default_upgrade_url_for_site("github"),
            DEFAULT_UPGRADE_URL_COM
        );
    }
}
