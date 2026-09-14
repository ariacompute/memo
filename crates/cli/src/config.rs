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
    if let Ok(override_home) = std::env::var("ARIA_COMPUTE_HOME") {
        if !override_home.is_empty() {
            return Ok(PathBuf::from(override_home));
        }
    }
    let home = dirs::home_dir().ok_or_else(|| {
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
    serde_yaml::from_str(&raw).map_err(|e| io::Error::other(e.to_string()))
}

/// Persist the CLI config (creates `~/.ariacompute`).
pub fn save_cli_config(cfg: &MemoCliConfig) -> io::Result<PathBuf> {
    ensure_memo_home()?;
    let path = default_cli_config_path()?;
    let raw = serde_yaml::to_string(cfg).map_err(|e| io::Error::other(e.to_string()))?;
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
}
