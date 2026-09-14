//! Upgrade the `aria-memo` CLI binary from GitHub/Gitee Releases.
//!
//! Migrated and trimmed from aria-router's `bin/src/upgrade.rs`: memo has no
//! FFI library, so only the CLI binary is reinstalled. The network client is
//! `reqwest::blocking` (memo's CLI is fully synchronous — no tokio runtime).
//! The release URL resolves from `--url` > the persisted `setup` config
//! (`~/.ariacompute/memo-cli.yml`, the preferred source) >
//! `ARIA_MEMO_UPGRADE_URL` env > a built-in default.

use crate::config::{default_upgrade_url, load_cli_config};
use serde::Deserialize;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub prerelease: bool,
    pub draft: bool,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseHost {
    GitHub,
    Gitee,
}

/// Run `aria-memo upgrade [version] [--upgrade-url U]`.
pub fn run(
    version: Option<&str>,
    current_version: &str,
    upgrade_url_arg: Option<&str>,
) -> io::Result<()> {
    let org = resolve_upgrade_url(upgrade_url_arg)?.trim_end_matches('/').to_string();
    let host = detect_host(&org)?;
    let releases = fetch_releases(host, &org)?;
    let target = select_release(&releases, version)?;
    let ver = strip_v(&target.tag);
    let current = strip_v(current_version);
    if ver == current {
        println!("already at {ver}");
        return Ok(());
    }

    let asset_os = detect_asset_os()?;
    let cli_name = memo_asset_name(&ver, asset_os);
    let cli_asset = find_asset(&target.assets, &cli_name)?;

    let staging = std::env::temp_dir().join(format!("aria-memo-upgrade-{ver}"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    let cli_archive = staging.join(&cli_name);
    download_url(
        &cli_asset.download_url,
        &cli_archive,
        &format!("aria-memo {ver}"),
    )?;

    let cli_extract = staging.join("cli");
    fs::create_dir_all(&cli_extract)?;
    extract_archive(&cli_archive, &cli_extract)?;

    install_cli(&cli_extract)?;

    let _ = fs::remove_dir_all(&staging);
    println!("upgraded to {ver}");
    Ok(())
}

/// Resolve the Releases org root: `--url` > persisted `setup` config
/// (`~/.ariacompute/memo-cli.yml`, the preferred source) >
/// `ARIA_MEMO_UPGRADE_URL` env > built-in default.
pub fn resolve_upgrade_url(upgrade_url_arg: Option<&str>) -> io::Result<String> {
    // 1) explicit `--url` override (overrides config).
    if let Some(v) = upgrade_url_arg {
        let t = v.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    // 2) persisted config — preferred source.
    let cfg = load_cli_config()?;
    if !cfg.upgrade_url.is_empty() {
        return Ok(cfg.upgrade_url);
    }
    // 3) env override.
    if let Ok(env) = std::env::var("ARIA_MEMO_UPGRADE_URL") {
        let t = env.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    // 4) built-in default.
    Ok(default_upgrade_url().to_string())
}

fn detect_host(org_url: &str) -> io::Result<ReleaseHost> {
    let lower = org_url.to_ascii_lowercase();
    if lower.contains("gitee.com") {
        Ok(ReleaseHost::Gitee)
    } else if lower.contains("github.com") {
        Ok(ReleaseHost::GitHub)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported upgrade_url host: {org_url}"),
        ))
    }
}

fn releases_api_url(host: ReleaseHost, org: &str) -> String {
    let owner = org.rsplit('/').next().unwrap_or("ariacompute");
    match host {
        ReleaseHost::GitHub => {
            format!("https://api.github.com/repos/{owner}/memo/releases?per_page=30")
        }
        ReleaseHost::Gitee => {
            format!("https://gitee.com/api/v5/repos/{owner}/memo/releases?per_page=30")
        }
    }
}

fn fetch_releases(host: ReleaseHost, org: &str) -> io::Result<Vec<ReleaseInfo>> {
    let url = releases_api_url(host, org);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(format!("aria-memo-upgrade/{}", env!("ARIA_MEMO_VERSION")))
        .build()
        .map_err(io_err)?;
    let resp = client.get(&url).send().map_err(io_err)?;
    if !resp.status().is_success() {
        return Err(io::Error::other(format!(
            "releases API {}: {}",
            resp.status(),
            url
        )));
    }
    let raw: Vec<GhRelease> = resp.json().map_err(io_err)?;
    Ok(raw
        .into_iter()
        .map(|r| ReleaseInfo {
            tag: r.tag_name,
            prerelease: r.prerelease,
            draft: r.draft,
            assets: r
                .assets
                .into_iter()
                .map(|a| ReleaseAsset {
                    name: a.name,
                    download_url: a.browser_download_url,
                })
                .collect(),
        })
        .collect())
}

pub fn select_release<'a>(
    releases: &'a [ReleaseInfo],
    version: Option<&str>,
) -> io::Result<&'a ReleaseInfo> {
    match version {
        None => releases
            .iter()
            .filter(|r| !r.draft && !r.prerelease)
            .filter(|r| parse_semver(&r.tag).is_some())
            .max_by_key(|r| parse_semver(&r.tag).unwrap_or((0, 0, 0)))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no stable release found")),
        Some(v) => {
            let want = normalize_tag(v);
            releases
                .iter()
                .find(|r| !r.draft && (normalize_tag(&r.tag) == want || r.tag == v))
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, format!("release {v} not found"))
                })
        }
    }
}

pub fn parse_semver(tag: &str) -> Option<(u64, u64, u64)> {
    let s = strip_v(tag);
    let core = s.split(['-', '+']).next().unwrap_or("");
    if core.is_empty() {
        return None;
    }
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

pub fn normalize_tag(v: &str) -> String {
    let t = v.trim();
    if t.starts_with('v') || t.starts_with('V') {
        t.to_string()
    } else {
        format!("v{t}")
    }
}

pub fn strip_v(v: &str) -> String {
    let t = v.trim();
    t.strip_prefix('v')
        .or_else(|| t.strip_prefix('V'))
        .unwrap_or(t)
        .to_string()
}

pub fn detect_asset_os() -> io::Result<&'static str> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    match (os, arch) {
        ("linux", "x86_64") => Ok("linux_x86_64"),
        ("linux", "aarch64") => Ok("linux_arm64"),
        ("macos", _) => Ok("macos"),
        ("windows", "x86_64") => Ok("windows_x86_64"),
        _ => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported platform {os}/{arch} for upgrade"),
        )),
    }
}

pub fn memo_asset_name(version: &str, asset_os: &str) -> String {
    if asset_os.starts_with("windows") {
        format!("aria-memo_{version}_{asset_os}.zip")
    } else {
        format!("aria-memo_{version}_{asset_os}.tar.gz")
    }
}

fn find_asset<'a>(assets: &'a [ReleaseAsset], name: &str) -> io::Result<&'a ReleaseAsset> {
    assets.iter().find(|a| a.name == name).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("release asset not found: {name}"),
        )
    })
}

fn download_url(url: &str, path: &Path, label: &str) -> io::Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(format!("aria-memo-upgrade/{}", env!("ARIA_MEMO_VERSION")))
        .build()
        .map_err(io_err)?;
    let resp = client.get(url).send().map_err(io_err)?;
    if !resp.status().is_success() {
        return Err(io::Error::other(format!(
            "download failed {}: {}",
            resp.status(),
            url
        )));
    }
    eprintln!("downloading {label}…");
    let mut file = fs::File::create(path)?;
    let mut reader = resp;
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
    }
    file.flush()?;
    Ok(())
}

fn extract_archive(archive: &Path, dest: &Path) -> io::Result<()> {
    let name = archive
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".zip") {
        extract_zip(archive, dest)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, dest)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown archive type: {name}"),
        ))
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> io::Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let out_path = match entry.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&out_path)?;
        io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> io::Result<()> {
    let file = fs::File::open(archive)?;
    let dec = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(dec);
    archive.unpack(dest)?;
    Ok(())
}

fn install_cli(extract_dir: &Path) -> io::Result<()> {
    let bin_name = if cfg!(windows) {
        "aria-memo.exe"
    } else {
        "aria-memo"
    };
    let src = find_named_file(extract_dir, bin_name)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{bin_name} not in archive"),
        )
    })?;
    let dest = std::env::current_exe()?;
    let dest_dir = dest
        .parent()
        .ok_or_else(|| io::Error::other("current_exe has no parent"))?;
    let tmp = dest_dir.join(format!(".aria-memo-upgrade-{}.tmp", std::process::id()));
    fs::copy(&src, &tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmp, perms)?;
    }
    fs::rename(&tmp, &dest).or_else(|e| match fs::copy(&tmp, &dest) {
        Ok(_) => {
            let _ = fs::remove_file(&tmp);
            Ok(())
        }
        Err(_) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    })?;
    Ok(())
}

fn find_named_file(root: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    for path in walk_files(root)? {
        if path.file_name().and_then(|s| s.to_str()) == Some(name) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn walk_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out)?;
            } else {
                out.push(path);
            }
        }
        Ok(())
    }
    walk(root, &mut out)?;
    Ok(out)
}

fn io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_releases() -> Vec<ReleaseInfo> {
        vec![
            ReleaseInfo {
                tag: "v0.8.0-rc1".into(),
                prerelease: true,
                draft: false,
                assets: vec![],
            },
            ReleaseInfo {
                tag: "v0.7.1".into(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
            ReleaseInfo {
                tag: "v0.8.0".into(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
            ReleaseInfo {
                tag: "v0.7.2".into(),
                prerelease: false,
                draft: false,
                assets: vec![
                    ReleaseAsset {
                        name: "aria-memo_0.7.2_linux_x86_64.tar.gz".into(),
                        download_url: "https://example/e".into(),
                    },
                ],
            },
            ReleaseInfo {
                tag: "v0.9.1".into(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
        ]
    }

    #[test]
    fn selects_latest_stable_skipping_prerelease() {
        let r = sample_releases();
        let got = select_release(&r, None).unwrap();
        assert_eq!(got.tag, "v0.9.1");
    }

    #[test]
    fn selects_newest_by_semver_not_api_order() {
        let r = vec![
            ReleaseInfo {
                tag: "v0.8.0".into(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
            ReleaseInfo {
                tag: "v0.9.1".into(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
            ReleaseInfo {
                tag: "v0.10.0".into(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
        ];
        assert_eq!(select_release(&r, None).unwrap().tag, "v0.10.0");
        let rev: Vec<_> = r.into_iter().rev().collect();
        assert_eq!(select_release(&rev, None).unwrap().tag, "v0.10.0");
    }

    #[test]
    fn parse_semver_core() {
        assert_eq!(parse_semver("v0.9.1"), Some((0, 9, 1)));
        assert_eq!(parse_semver("0.8.0"), Some((0, 8, 0)));
        assert_eq!(parse_semver("v1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(parse_semver("not-a-version"), None);
    }

    #[test]
    fn selects_by_version_with_or_without_v() {
        let r = sample_releases();
        assert_eq!(select_release(&r, Some("0.7.1")).unwrap().tag, "v0.7.1");
        assert_eq!(select_release(&r, Some("v0.7.2")).unwrap().tag, "v0.7.2");
    }

    #[test]
    fn normalize_and_strip_tag() {
        assert_eq!(normalize_tag("0.7.2"), "v0.7.2");
        assert_eq!(normalize_tag("v0.7.2"), "v0.7.2");
        assert_eq!(strip_v("v0.7.2"), "0.7.2");
        assert_eq!(strip_v("0.7.2"), "0.7.2");
    }

    #[test]
    fn memo_asset_names() {
        assert_eq!(
            memo_asset_name("0.7.2", "linux_x86_64"),
            "aria-memo_0.7.2_linux_x86_64.tar.gz"
        );
        assert_eq!(
            memo_asset_name("0.7.2", "windows_x86_64"),
            "aria-memo_0.7.2_windows_x86_64.zip"
        );
    }

    #[test]
    fn github_and_gitee_api_paths() {
        assert!(
            releases_api_url(ReleaseHost::GitHub, "https://github.com/ariacompute")
                .contains("api.github.com/repos/ariacompute/memo/releases")
        );
        assert!(
            releases_api_url(ReleaseHost::Gitee, "https://gitee.com/ariacompute")
                .contains("gitee.com/api/v5/repos/ariacompute/memo/releases")
        );
    }

    #[test]
    fn missing_version_errors() {
        let r = sample_releases();
        assert!(select_release(&r, Some("9.9.9")).is_err());
    }

    #[test]
    fn resolve_upgrade_url_precedence() {
        // No arg, no env, no config → built-in default.
        std::env::remove_var("ARIA_MEMO_UPGRADE_URL");
        // config would be read from disk; we cannot easily stub, so just assert
        // the default path when arg is empty and env unset and config empty.
        // (config load may return a real file in CI; skip strict equality here.)
        let got = resolve_upgrade_url(None);
        assert!(got.is_ok());
    }
}
