//! Upgrade the `aria-memo` CLI binary from GitHub/Gitee Releases.
//!
//! Migrated and trimmed from aria-router's `bin/src/upgrade.rs`: memo has no
//! FFI library, so only the CLI binary is reinstalled. The network client is
//! `reqwest::blocking` (memo's CLI is fully synchronous — no tokio runtime).
//! The release URL resolves from `--url` > the persisted `setup` config
//! (`~/.ariacompute/memo-cli.yml`, the preferred source) >
//! `ARIA_MEMO_UPGRADE_URL` env > a built-in default.
//!
//! The HTTP egress sits behind [`HttpGet`] so the whole download → extract →
//! install pipeline can be exercised in-process with a fake source; only
//! [`ReqwestGet::get`] (real network) is excluded from coverage.

use crate::config::{default_upgrade_url, load_cli_config};
use serde::Deserialize;
use std::fs;
use std::io::{self, Write};
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

/// Name of the release binary inside the archive (platform specific).
#[cfg(windows)]
const BIN_NAME: &str = "aria-memo.exe";
#[cfg(not(windows))]
const BIN_NAME: &str = "aria-memo";

/// HTTP egress seam: production uses [`ReqwestGet`], tests inject a fake.
pub(crate) trait HttpGet {
    fn get(&self, url: &str) -> io::Result<Vec<u8>>;
}

/// Real network implementation (exercised in tests against a loopback server).
struct ReqwestGet;

impl HttpGet for ReqwestGet {
    fn get(&self, url: &str) -> io::Result<Vec<u8>> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(600))
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(format!("aria-memo-upgrade/{}", env!("ARIA_MEMO_VERSION")))
            .build()
            .map_err(io_err)?;
        let resp = client.get(url).send().map_err(io_err)?;
        if !resp.status().is_success() {
            return Err(io::Error::other(format!(
                "request failed {}: {}",
                resp.status(),
                url
            )));
        }
        let body = resp.bytes().map_err(io_err)?;
        Ok(body.to_vec())
    }
}

/// Everything `run_with` needs, so the pipeline is independent of process state.
pub(crate) struct UpgradeRequest<'a> {
    pub version: Option<&'a str>,
    pub current_version: &'a str,
    pub org: &'a str,
    pub staging_root: &'a Path,
    pub install_dest: &'a Path,
}

/// Run `aria-memo upgrade [version] [--url U]`.
pub fn run(
    version: Option<&str>,
    current_version: &str,
    upgrade_url_arg: Option<&str>,
) -> io::Result<()> {
    let org = resolve_upgrade_url(upgrade_url_arg)?;
    let install_dest = install_dir()?;
    let staging_root = std::env::temp_dir();
    run_with(
        &UpgradeRequest {
            version,
            current_version,
            org: &org,
            staging_root: &staging_root,
            install_dest: &install_dest,
        },
        &ReqwestGet,
    )
}

/// Download → extract → install pipeline, driven by an injected HTTP source.
pub(crate) fn run_with(req: &UpgradeRequest<'_>, http: &dyn HttpGet) -> io::Result<()> {
    let org = req.org.trim_end_matches('/');
    let host = detect_host(org)?;
    let releases = fetch_releases(http, host, org)?;
    let target = select_release(&releases, req.version)?;
    let ver = strip_v(&target.tag);
    if ver == strip_v(req.current_version) {
        println!("already at {ver}");
        return Ok(());
    }

    let asset_os = detect_asset_os()?;
    let cli_name = memo_asset_name(&ver, asset_os);
    let cli_asset = find_asset(&target.assets, &cli_name)?;

    let staging = req.staging_root.join(format!("aria-memo-upgrade-{ver}"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    let cli_archive = staging.join(&cli_name);
    download_url(
        http,
        &cli_asset.download_url,
        &cli_archive,
        &format!("aria-memo {ver}"),
    )?;

    let cli_extract = staging.join("cli");
    fs::create_dir_all(&cli_extract)?;
    extract_archive(&cli_archive, &cli_extract)?;

    install_cli_into(&cli_extract, &req.install_dest.join(BIN_NAME))?;

    let _ = fs::remove_dir_all(&staging);
    println!("upgraded to {ver}");
    Ok(())
}

/// Resolve the Releases org root: `--url` > persisted `setup` config
/// (`~/.ariacompute/memo-cli.yml`, the preferred source) >
/// `ARIA_MEMO_UPGRADE_URL` env > built-in default.
pub fn resolve_upgrade_url(upgrade_url_arg: Option<&str>) -> io::Result<String> {
    let cfg = load_cli_config()?;
    let env_url = std::env::var("ARIA_MEMO_UPGRADE_URL").ok();
    Ok(resolve_upgrade_url_with(
        upgrade_url_arg,
        &cfg.upgrade_url,
        env_url.as_deref(),
    ))
}

fn resolve_upgrade_url_with(
    upgrade_url_arg: Option<&str>,
    config_url: &str,
    env_url: Option<&str>,
) -> String {
    // 1) explicit `--url` override (overrides config).
    if let Some(v) = upgrade_url_arg {
        let t = v.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    // 2) persisted config — preferred source.
    if !config_url.is_empty() {
        return config_url.to_string();
    }
    // 3) env override.
    if let Some(env) = env_url {
        let t = env.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    // 4) built-in default.
    default_upgrade_url().to_string()
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
    let owner = org
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("ariacompute");
    match host {
        ReleaseHost::GitHub => {
            format!("https://api.github.com/repos/{owner}/memo/releases?per_page=30")
        }
        ReleaseHost::Gitee => {
            format!("https://gitee.com/api/v5/repos/{owner}/memo/releases?per_page=30")
        }
    }
}

fn fetch_releases(
    http: &dyn HttpGet,
    host: ReleaseHost,
    org: &str,
) -> io::Result<Vec<ReleaseInfo>> {
    let url = releases_api_url(host, org);
    let body = http.get(&url)?;
    let raw: Vec<GhRelease> = serde_json::from_slice(&body).map_err(io_err)?;
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
    let core = match s.find(['-', '+']) {
        Some(i) => &s[..i],
        None => s.as_str(),
    };
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
    asset_os_for(std::env::consts::OS, std::env::consts::ARCH)
}

fn asset_os_for(os: &str, arch: &str) -> io::Result<&'static str> {
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

fn download_url(http: &dyn HttpGet, url: &str, path: &Path, label: &str) -> io::Result<()> {
    let body = http.get(url)?;
    eprintln!("downloading {label}…");
    let mut file = fs::File::create(path)?;
    file.write_all(&body)?;
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

/// Map a zip reader error to `InvalidData` (never silently truncated archives).
fn invalid_archive(e: zip::result::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

fn extract_zip(archive: &Path, dest: &Path) -> io::Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(invalid_archive)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(invalid_archive)?;
        let out_path = match entry.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        // Entry paths always have a parent (`dest/<name>`), default to `dest`.
        fs::create_dir_all(out_path.parent().unwrap_or(dest))?;
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

fn install_cli_into(extract_dir: &Path, dest: &Path) -> io::Result<()> {
    let src = find_named_file(extract_dir, BIN_NAME)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{BIN_NAME} not in archive"),
        )
    })?;
    let dest_dir = dest
        .parent()
        .ok_or_else(|| io::Error::other("install destination has no parent"))?;
    let tmp = dest_dir.join(format!(".aria-memo-upgrade-{}.tmp", std::process::id()));
    fs::copy(&src, &tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmp, perms)?;
    }
    replace_file(&tmp, dest)
}

/// Replace `dest` with `tmp`, falling back to a copy when `rename` fails
/// (e.g. `EXDEV` on a different filesystem). Returns the rename error if the
/// copy also fails, so callers never silently keep the old binary.
fn replace_file(tmp: &Path, dest: &Path) -> io::Result<()> {
    replace_file_with(tmp, dest, |a, b| fs::rename(a, b))
}

type RenameFn = fn(&Path, &Path) -> io::Result<()>;

fn replace_file_with(tmp: &Path, dest: &Path, rename: RenameFn) -> io::Result<()> {
    rename(tmp, dest).or_else(|e| match fs::copy(tmp, dest) {
        Ok(_) => {
            let _ = fs::remove_file(tmp);
            Ok(())
        }
        Err(_) => {
            let _ = fs::remove_file(tmp);
            Err(e)
        }
    })
}

/// Directory of the currently running binary (the install target).
fn install_dir() -> io::Result<PathBuf> {
    install_dir_from(&std::env::current_exe()?)
}

fn install_dir_from(exe: &Path) -> io::Result<PathBuf> {
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("current_exe has no parent"))
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
    use std::collections::HashMap;
    use std::io::Read;

    /// Fake HTTP source: URL → body (or an error kind/message).
    #[derive(Default)]
    struct FakeHttp {
        routes: HashMap<String, Result<Vec<u8>, (io::ErrorKind, String)>>,
    }

    impl FakeHttp {
        fn with(mut self, url: &str, body: Vec<u8>) -> Self {
            self.routes.insert(url.to_string(), Ok(body));
            self
        }

        fn failing(mut self, url: &str, kind: io::ErrorKind, msg: &str) -> Self {
            self.routes
                .insert(url.to_string(), Err((kind, msg.to_string())));
            self
        }
    }

    impl HttpGet for FakeHttp {
        fn get(&self, url: &str) -> io::Result<Vec<u8>> {
            match self.routes.get(url) {
                Some(Ok(body)) => Ok(body.clone()),
                Some(Err((kind, msg))) => Err(io::Error::new(*kind, msg.clone())),
                None => Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("no route for {url}"),
                )),
            }
        }
    }

    fn releases_json(entries: &[(&str, bool, &str)]) -> Vec<u8> {
        // (tag, prerelease, asset_name_or_empty)
        let arr: Vec<serde_json::Value> = entries
            .iter()
            .map(|(tag, prerelease, asset)| {
                let assets = if asset.is_empty() {
                    serde_json::json!([])
                } else {
                    serde_json::json!([{
                        "name": asset,
                        "browser_download_url": format!("https://dl.example/{asset}")
                    }])
                };
                serde_json::json!({
                    "tag_name": tag,
                    "draft": false,
                    "prerelease": prerelease,
                    "assets": assets
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!(arr)).unwrap()
    }

    fn releases_url(org: &str) -> String {
        releases_api_url(detect_host(org).unwrap(), org)
    }

    fn download_url_for(asset: &str) -> String {
        format!("https://dl.example/{asset}")
    }

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
                assets: vec![ReleaseAsset {
                    name: "aria-memo_0.7.2_linux_x86_64.tar.gz".into(),
                    download_url: "https://example/e".into(),
                }],
            },
            ReleaseInfo {
                tag: "v0.9.1".into(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
        ]
    }

    /// Build a `.tar.gz` containing the given `(path, bytes)` entries.
    fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, *name, &data[..]).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    /// Build a `.zip` containing the given dirs and `(path, bytes)` entries.
    fn zip_bytes(dirs: &[&str], entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for d in dirs {
            writer.add_directory(*d, opts).unwrap();
        }
        for (name, data) in entries {
            writer.start_file(*name, opts).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aria-memo-upgrade-test-{}-{tag}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A request rooted at fresh temporary staging/install dirs.
    struct Fixture {
        root: PathBuf,
        staging: PathBuf,
        install: PathBuf,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let root = temp_dir(tag);
            let staging = root.join("staging");
            let install = root.join("install");
            fs::create_dir_all(&staging).unwrap();
            fs::create_dir_all(&install).unwrap();
            Self {
                root,
                staging,
                install,
            }
        }

        fn request<'a>(
            &'a self,
            version: Option<&'a str>,
            current: &'a str,
            org: &'a str,
        ) -> UpgradeRequest<'a> {
            UpgradeRequest {
                version,
                current_version: current,
                org,
                staging_root: &self.staging,
                install_dest: &self.install,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn selects_latest_stable_skipping_prerelease() {
        let r = sample_releases();
        assert_eq!(select_release(&r, None).unwrap().tag, "v0.9.1");
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
        assert_eq!(parse_semver("v1.2.3+build7"), Some((1, 2, 3)));
        assert_eq!(parse_semver("not-a-version"), None);
        // Version with a missing minor/patch defaults to 0.
        assert_eq!(parse_semver("2"), Some((2, 0, 0)));
        // Empty core after a leading pre-release marker is invalid.
        assert_eq!(parse_semver("-rc1"), None);
    }

    #[test]
    fn selects_latest_ignores_drafts_and_unparseable_tags() {
        let r = vec![
            ReleaseInfo {
                tag: "v9.9.9".into(),
                prerelease: false,
                draft: true,
                assets: vec![],
            },
            ReleaseInfo {
                tag: "nightly".into(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
            ReleaseInfo {
                tag: "v1.0.0".into(),
                prerelease: false,
                draft: false,
                assets: vec![],
            },
        ];
        assert_eq!(select_release(&r, None).unwrap().tag, "v1.0.0");
        // Nothing eligible at all.
        assert!(select_release(&[], None).is_err());
        // Drafts are never selectable by explicit version either.
        assert!(select_release(&r, Some("9.9.9")).is_err());
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
        assert_eq!(normalize_tag("V0.7.2"), "V0.7.2");
        assert_eq!(strip_v("v0.7.2"), "0.7.2");
        assert_eq!(strip_v("V0.7.2"), "0.7.2");
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
        // Trailing slash / empty owner falls back to the org default.
        assert!(releases_api_url(ReleaseHost::GitHub, "https://github.com/")
            .contains("repos/ariacompute/memo"));
    }

    #[test]
    fn missing_version_errors() {
        let r = sample_releases();
        assert!(select_release(&r, Some("9.9.9")).is_err());
    }

    #[test]
    fn resolve_upgrade_url_precedence() {
        // Explicit argument always wins.
        assert_eq!(
            resolve_upgrade_url_with(Some(" https://gitee.com/a "), "cfg", Some("env")),
            "https://gitee.com/a"
        );
        // Blank argument falls through to config.
        assert_eq!(
            resolve_upgrade_url_with(Some("   "), "https://github.com/cfg", Some("env")),
            "https://github.com/cfg"
        );
        // Then env.
        assert_eq!(
            resolve_upgrade_url_with(None, "", Some("https://github.com/env")),
            "https://github.com/env"
        );
        // Blank env falls through to the built-in default.
        assert_eq!(
            resolve_upgrade_url_with(None, "", Some("  ")),
            crate::config::DEFAULT_UPGRADE_URL_COM
        );
        assert_eq!(
            resolve_upgrade_url_with(None, "", None),
            crate::config::DEFAULT_UPGRADE_URL_COM
        );
        // The public entrypoint resolves from the on-disk config/env and returns Ok.
        assert!(resolve_upgrade_url(Some("https://github.com/ariacompute")).is_ok());
    }

    #[test]
    fn detect_host_routes_gitee_github_unsupported() {
        assert_eq!(
            detect_host("https://github.com/ariacompute").unwrap(),
            ReleaseHost::GitHub
        );
        assert_eq!(
            detect_host("https://gitee.com/ariacompute").unwrap(),
            ReleaseHost::Gitee
        );
        // Unknown host (neither github nor gitee) is rejected.
        assert!(detect_host("https://example.com/x").is_err());
    }

    #[test]
    fn find_asset_missing_errors() {
        let assets = vec![ReleaseAsset {
            name: "a".into(),
            download_url: "u".into(),
        }];
        assert!(find_asset(&assets, "b").is_err());
        assert!(find_asset(&assets, "a").is_ok());
    }

    #[test]
    fn asset_os_covers_every_supported_platform() {
        assert_eq!(asset_os_for("linux", "x86_64").unwrap(), "linux_x86_64");
        assert_eq!(asset_os_for("linux", "aarch64").unwrap(), "linux_arm64");
        assert_eq!(asset_os_for("macos", "x86_64").unwrap(), "macos");
        assert_eq!(asset_os_for("macos", "aarch64").unwrap(), "macos");
        assert_eq!(asset_os_for("windows", "x86_64").unwrap(), "windows_x86_64");
        assert!(asset_os_for("freebsd", "riscv64").is_err());
        // The wrapper reflects the host platform.
        assert!(detect_asset_os().is_ok());
    }

    #[test]
    fn install_dir_from_rejects_rootless_path() {
        assert!(install_dir_from(Path::new("/usr/bin/aria-memo")).is_ok());
        assert!(install_dir_from(Path::new("/")).is_err());
        assert!(install_dir().is_ok());
    }

    #[test]
    fn fetch_releases_parses_and_reports_errors() {
        let org = "https://github.com/ariacompute";
        let url = releases_url(org);
        let body = releases_json(&[("v1.0.0", false, "aria-memo_1.0.0_linux_x86_64.tar.gz")]);
        let http = FakeHttp::default().with(&url, body);
        let got = fetch_releases(&http, ReleaseHost::GitHub, org).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].tag, "v1.0.0");
        assert_eq!(
            got[0].assets[0].download_url,
            download_url_for("aria-memo_1.0.0_linux_x86_64.tar.gz")
        );

        // Transport failure propagates.
        let broken = FakeHttp::default().failing(&url, io::ErrorKind::ConnectionReset, "reset");
        assert!(fetch_releases(&broken, ReleaseHost::GitHub, org).is_err());

        // Malformed JSON is an error, never an empty release list.
        let garbage = FakeHttp::default().with(&url, b"not json".to_vec());
        assert!(fetch_releases(&garbage, ReleaseHost::GitHub, org).is_err());
    }

    #[test]
    fn run_with_reports_already_current_version() {
        let fx = Fixture::new("already");
        let org = "https://github.com/ariacompute";
        let http =
            FakeHttp::default().with(&releases_url(org), releases_json(&[("v0.9.0", false, "")]));
        let req = fx.request(None, "v0.9.0", org);
        run_with(&req, &http).unwrap();
        // No download route was registered: reaching one would have errored.
        assert!(fs::read_dir(&fx.install).unwrap().next().is_none());
    }

    #[test]
    fn run_with_downloads_extracts_and_installs() {
        let fx = Fixture::new("success");
        let org = "https://github.com/ariacompute";
        let ver = "0.9.1";
        let asset = memo_asset_name(ver, detect_asset_os().unwrap());
        let archive = tar_gz(&[("bin/aria-memo", b"#!/bin/sh\nprintf hi\n")]);
        let http = FakeHttp::default()
            .with(
                &releases_url(org),
                releases_json(&[("v0.9.1", false, &asset)]),
            )
            .with(&download_url_for(&asset), archive);

        // Pre-existing staging dir must be cleared before the run.
        let stale = fx.staging.join(format!("aria-memo-upgrade-{ver}"));
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("stale.txt"), b"junk").unwrap();

        let req = fx.request(None, "0.1.0", org);
        run_with(&req, &http).unwrap();

        let installed = fx.install.join(BIN_NAME);
        let mut content = Vec::new();
        fs::File::open(&installed)
            .unwrap()
            .read_to_end(&mut content)
            .unwrap();
        assert_eq!(content, b"#!/bin/sh\nprintf hi\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&installed).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        // Staging is cleaned up.
        assert!(!stale.exists());
    }

    #[test]
    fn run_with_rejects_unsupported_host_before_network() {
        let fx = Fixture::new("badhost");
        let req = fx.request(None, "0.1.0", "https://example.com/ariacompute");
        assert!(run_with(&req, &FakeHttp::default()).is_err());
    }

    #[test]
    fn run_with_missing_asset_or_bad_archive_errors() {
        let fx = Fixture::new("missing-asset");
        let org = "https://github.com/ariacompute";
        let req = fx.request(None, "0.1.0", org);
        // Release exists but has no matching asset.
        let http =
            FakeHttp::default().with(&releases_url(org), releases_json(&[("v0.9.1", false, "")]));
        assert!(run_with(&req, &http).is_err());

        // Asset present, archive type unknown -> extract error.
        let fx2 = Fixture::new("bad-archive");
        let asset = memo_asset_name("0.9.1", detect_asset_os().unwrap());
        let renamed = asset.replace(".tar.gz", ".bin").replace(".zip", ".bin");
        let http2 = FakeHttp::default()
            .with(
                &releases_url(org),
                releases_json(&[("v0.9.1", false, &renamed)]),
            )
            .with(&download_url_for(&renamed), b"whatever".to_vec());
        let req2 = fx2.request(None, "0.1.0", org);
        assert!(run_with(&req2, &http2).is_err());
    }

    #[test]
    fn run_with_download_failure_errors() {
        let fx = Fixture::new("dl-fail");
        let org = "https://github.com/ariacompute";
        let asset = memo_asset_name("0.9.1", detect_asset_os().unwrap());
        let http = FakeHttp::default()
            .with(
                &releases_url(org),
                releases_json(&[("v0.9.1", false, &asset)]),
            )
            .failing(
                &download_url_for(&asset),
                io::ErrorKind::TimedOut,
                "timeout",
            );
        let req = fx.request(None, "0.1.0", org);
        assert!(run_with(&req, &http).is_err());
        assert!(fs::read_dir(&fx.install).unwrap().next().is_none());
    }

    #[test]
    fn run_with_archive_without_binary_errors() {
        let fx = Fixture::new("no-bin");
        let org = "https://github.com/ariacompute";
        let asset = memo_asset_name("0.9.1", detect_asset_os().unwrap());
        let archive = tar_gz(&[("docs/README.md", b"no binary here")]);
        let http = FakeHttp::default()
            .with(
                &releases_url(org),
                releases_json(&[("v0.9.1", false, &asset)]),
            )
            .with(&download_url_for(&asset), archive);
        let req = fx.request(None, "0.1.0", org);
        assert!(run_with(&req, &http).is_err());
        assert!(fs::read_dir(&fx.install).unwrap().next().is_none());
    }

    #[test]
    fn run_entrypoint_propagates_errors() {
        // Unsupported host: fails during detect_host, before any network call.
        assert!(run(None, "0.1.0", Some("https://example.com/ariacompute")).is_err());
    }

    #[test]
    fn extract_zip_writes_nested_files_and_skips_unsafe_names() {
        let dir = temp_dir("zip");
        let archive = dir.join("a.zip");
        fs::write(
            &archive,
            zip_bytes(
                &["nested/"],
                &[("nested/aria-memo", b"bin"), ("../escape.txt", b"nope")],
            ),
        )
        .unwrap();
        let dest = dir.join("out");
        fs::create_dir_all(&dest).unwrap();
        extract_archive(&archive, &dest).unwrap();
        assert_eq!(fs::read(dest.join("nested/aria-memo")).unwrap(), b"bin");
        // Path traversal entries are dropped rather than written outside `dest`.
        assert!(!dir.join("escape.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_rejects_corrupt_zip_archives() {
        let dir = temp_dir("zip-corrupt");
        let dest = dir.join("out");
        fs::create_dir_all(&dest).unwrap();

        // Not a zip at all -> archive open error.
        let bad = dir.join("bad.zip");
        fs::write(&bad, b"definitely not a zip").unwrap();
        assert!(extract_archive(&bad, &dest).is_err());

        // Valid central directory, clobbered local header -> per-entry error.
        let mut bytes = zip_bytes(&[], &[("aria-memo", b"bin")]);
        let local_sig = [b'P', b'K', 3, 4];
        let pos = bytes
            .windows(4)
            .position(|w| w == local_sig)
            .expect("local file header signature");
        bytes[pos..pos + 4].copy_from_slice(&[0, 0, 0, 0]);
        let corrupt = dir.join("corrupt.zip");
        fs::write(&corrupt, bytes).unwrap();
        assert!(extract_archive(&corrupt, &dest).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_archive_maps_to_invalid_data() {
        let e = invalid_archive(zip::result::ZipError::InvalidArchive("bad header"));
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert!(e.to_string().contains("bad header"));
    }

    #[test]
    fn extract_rejects_unknown_archive_and_missing_file() {
        let dir = temp_dir("unknown");
        let dest = dir.join("out");
        fs::create_dir_all(&dest).unwrap();
        // Unknown extension.
        let bin = dir.join("payload.bin");
        fs::write(&bin, b"x").unwrap();
        assert!(extract_archive(&bin, &dest).is_err());
        // Path without a file name.
        assert!(extract_archive(Path::new("/"), &dest).is_err());
        // Known extension but the file does not exist.
        assert!(extract_archive(&dir.join("missing.tar.gz"), &dest).is_err());
        assert!(extract_archive(&dir.join("missing.zip"), &dest).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_tar_gz_unpacks_entries() {
        let dir = temp_dir("tar");
        let archive = dir.join("a.tar.gz");
        fs::write(&archive, tar_gz(&[("aria-memo", b"bin")])).unwrap();
        let dest = dir.join("out");
        fs::create_dir_all(&dest).unwrap();
        extract_archive(&archive, &dest).unwrap();
        assert_eq!(fs::read(dest.join("aria-memo")).unwrap(), b"bin");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_requires_the_binary_and_a_parent_path() {
        let dir = temp_dir("install-errors");
        let empty = dir.join("empty");
        fs::create_dir_all(&empty).unwrap();
        // Archive without the binary.
        assert!(install_cli_into(&empty, &dir.join(BIN_NAME)).is_err());
        // Binary present but the destination has no parent directory.
        fs::write(empty.join(BIN_NAME), b"bin").unwrap();
        assert!(install_cli_into(&empty, Path::new("/")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_file_falls_back_to_copy_when_rename_fails() {
        let dir = temp_dir("replace");
        let tmp = dir.join("tmp.bin");
        let dest = dir.join("dest.bin");
        fs::write(&tmp, b"new").unwrap();
        // Simulate EXDEV: rename fails, copy succeeds.
        replace_file_with(&tmp, &dest, |_, _| Err(io::Error::other("exdev"))).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        assert!(!tmp.exists());
        // Both rename and copy fail -> the rename error is surfaced.
        let tmp2 = dir.join("tmp2.bin");
        fs::write(&tmp2, b"new").unwrap();
        let bad_dest = dir.join("missing").join("dest.bin");
        let err =
            replace_file_with(&tmp2, &bad_dest, |_, _| Err(io::Error::other("exdev"))).unwrap_err();
        assert!(err.to_string().contains("exdev"));
        assert!(!tmp2.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn walk_files_reports_missing_directories() {
        let dir = temp_dir("walk");
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/file.txt"), b"x").unwrap();
        let files = walk_files(&dir).unwrap();
        assert_eq!(files.len(), 1);
        assert!(find_named_file(&dir, "file.txt").unwrap().is_some());
        assert!(find_named_file(&dir, "absent").unwrap().is_none());
        assert!(walk_files(&dir.join("missing")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Serve exactly one HTTP/1.1 request on a loopback port and return its URL.
    fn serve_once(status: &str, body: &[u8]) -> (String, std::thread::JoinHandle<()>) {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let status = status.to_string();
        let body = body.to_vec();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        });
        (format!("http://{addr}/payload"), handle)
    }

    #[test]
    fn reqwest_source_fetches_and_reports_http_and_transport_errors() {
        let (url, server) = serve_once("200 OK", b"release-bytes");
        assert_eq!(ReqwestGet.get(&url).unwrap(), b"release-bytes");
        server.join().unwrap();

        // Non-2xx status is surfaced with the URL.
        let (url, server) = serve_once("503 Service Unavailable", b"");
        let err = ReqwestGet.get(&url).unwrap_err();
        assert!(err.to_string().contains("503"), "got {err}");
        server.join().unwrap();

        // A closed port fails at the transport layer.
        let closed = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            drop(listener);
            format!("http://{addr}/payload")
        };
        assert!(ReqwestGet.get(&closed).is_err());
    }

    #[test]
    fn download_writes_bytes_and_propagates_io_errors() {
        let dir = temp_dir("download");
        let out = dir.join("payload");
        let http = FakeHttp::default().with("https://dl.example/x", b"body".to_vec());
        download_url(&http, "https://dl.example/x", &out, "test").unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"body");
        // Missing route -> transport error.
        assert!(download_url(&http, "https://dl.example/missing", &out, "test").is_err());
        // Unwritable destination directory.
        let bad = dir.join("no-such-dir").join("payload");
        assert!(download_url(&http, "https://dl.example/x", &bad, "test").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
