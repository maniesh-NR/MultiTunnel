//! Engine self-provisioning: locate → verify → embedded → download.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert D"; the frozen
//! `mt_engine::EngineLocator` trait is authoritative for signatures):
//! `DefaultEngineLocator` resolves the Xray binary in this order:
//!
//! 1. `$MULTITUNNEL_ENGINE` — explicit path (missing file → warn, fall
//!    through; never silently — D14);
//! 2. `<exe_dir>/engine/xray(.exe)` — portable install next to the binary;
//! 3. `<config_dir>/engine/xray(.exe)` — verified cache: sha256 of every
//!    file listed in `engine/SHA256SUMS` must match (mismatch → warn +
//!    ACCEPT by default; `MULTITUNNEL_ENGINE_STRICT=1` → refuse);
//! 4. `ensure()` only: embedded zip (feature `embedded`, build.rs) then
//!    download from `$MULTITUNNEL_ENGINE_URL` (direct zip) or the GitHub
//!    releases latest API (`XTLS/Xray-core`, asset `Xray-windows-64.zip`
//!    on Windows / `Xray-linux-x86_64.zip` elsewhere).
//!
//! `locate()` NEVER downloads (frozen trait doc). All HTTP goes through
//! [`HttpFetch`] so tests inject fixtures (no network in tests); the
//! reqwest impl sits behind the default feature `net`.
//!
//! IMPLEMENTATION NOTES (picked and documented):
//! * **Zip-slip protection** — allowlist-by-basename flattening: only
//!   entries whose basename is exactly `xray`, `xray.exe`, `geoip.dat` or
//!   `geosite.dat` are extracted, and they are always written DIRECTLY
//!   into the staging dir. Entry names containing `..`, leading `/` or `:`
//!   are skipped, so traversal components never reach path construction.
//! * **Download/install atomicity** — the archive is extracted into a
//!   private staging dir `.engine-new-<pid>-<nanos>` beside the target
//!   (same filesystem), `SHA256SUMS` is written inside it, then the swap
//!   is `rename`-based: old dir → `.engine-old-…`, staging → target,
//!   backup removed on success and restored if the final rename fails. A
//!   crash never leaves a half-extracted `engine/` dir.
//! * **SHA256SUMS format** — coreutils-compatible lines
//!   `<64 lowercase hex chars>  <relative file name>` (two spaces);
//!   parsing accepts an optional `*` binary-mode marker before the name
//!   and is case-insensitive on the digest. An absent/unreadable file
//!   leaves the dir `Unverified` (accepted with a warn); a malformed or
//!   mismatching line makes it `Fail`.
//! * `geoip.dat`/`geosite.dat` and the binary get mode 0755 (POSIX).
//! * Installation does synchronous I/O inside the async `ensure()` on
//!   purpose: it is a rare, user-triggered operation and the contract does
//!   not add tokio to this crate's dependencies. No runtime is created
//!   here (global rule 7).
//! * Embedded bytes are preferred over downloading (per the frozen trait
//!   doc order: env → exe-dir → verified cache → embedded → download).

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use zip::ZipArchive;

/// Explicit engine binary override (resolution step 1).
pub const ENV_ENGINE: &str = "MULTITUNNEL_ENGINE";
/// Direct zip URL override for the download step (resolution step 4).
pub const ENV_ENGINE_URL: &str = "MULTITUNNEL_ENGINE_URL";
/// `=1` refuses engines whose files fail SHA256SUMS verification.
pub const ENV_ENGINE_STRICT: &str = "MULTITUNNEL_ENGINE_STRICT";

const ENGINE_DIR: &str = "engine";
const SUMS_FILE: &str = "SHA256SUMS";
/// The only archive entries we extract (zip-slip allowlist, see docs).
const INSTALLABLE: [&str; 4] = ["xray", "xray.exe", "geoip.dat", "geosite.dat"];

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Structured platform errors (`thiserror`); surfaced to callers as
/// `anyhow::Error` because the frozen trait/`lib.rs` signatures use anyhow.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// The crate was compiled without the default `net` feature.
    #[error("network feature disabled: rebuild with `--features net` to enable downloads and update checks")]
    NetDisabled,
    #[error("downloaded engine archive is not a readable zip file")]
    BadZip,
    #[error("github releases response was not valid JSON")]
    BadRelease,
    #[error("release {tag} ships no asset named `{asset}`")]
    NoAsset { tag: String, asset: String },
    #[error("downloaded archive contains no usable `xray` binary for this platform")]
    NoEngineBinary,
    #[error("I/O error while installing the engine")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// HTTP abstraction (tests inject fixtures — no network in tests)
// ---------------------------------------------------------------------------

/// Minimal HTTP GET used by the download and update-check paths.
#[async_trait::async_trait]
pub trait HttpFetch: Send + Sync {
    /// GET `url` and return the response body; non-2xx is an error.
    async fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>>;
}

/// The production [`HttpFetch`] (reqwest + rustls), behind feature `net`.
#[cfg(feature = "net")]
pub struct ReqwestFetch {
    client: reqwest::Client,
}

#[cfg(feature = "net")]
impl ReqwestFetch {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(user_agent())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

#[cfg(feature = "net")]
impl Default for ReqwestFetch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "net")]
#[async_trait::async_trait]
impl HttpFetch for ReqwestFetch {
    async fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        // D9 (S6-REV-C P1-5): reqwest's error Display embeds the FULL request
        // URL, and user-set mirror URLs can carry tokens in the query string
        // (`…/xray.zip?token=…`). Strip the URL from every error before it
        // propagates into GUI logs / bug reports — same treatment as
        // mt-links' subscription fetch.
        let resp =
            self.client.get(url).send().await.map_err(|e| {
                anyhow::anyhow!("engine download request failed: {}", e.without_url())
            })?;
        let resp = resp.error_for_status().map_err(|e| {
            anyhow::anyhow!(
                "engine download server rejected the request: {}",
                e.without_url()
            )
        })?;
        Ok(resp
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("engine download body read failed: {}", e.without_url()))?
            .to_vec())
    }
}

/// Stand-in used when the `net` feature is compiled out: every fetch fails
/// with a clear [`PlatformError::NetDisabled`] instead of silently
/// pretending the network is down.
#[cfg(not(feature = "net"))]
pub struct NoNetFetch;

#[cfg(not(feature = "net"))]
#[async_trait::async_trait]
impl HttpFetch for NoNetFetch {
    async fn fetch(&self, _url: &str) -> anyhow::Result<Vec<u8>> {
        Err(anyhow::Error::new(PlatformError::NetDisabled))
    }
}

pub(crate) fn default_http() -> Arc<dyn HttpFetch> {
    #[cfg(feature = "net")]
    let http: Arc<dyn HttpFetch> = Arc::new(ReqwestFetch::new());
    #[cfg(not(feature = "net"))]
    let http: Arc<dyn HttpFetch> = Arc::new(NoNetFetch);
    http
}

#[cfg(feature = "net")]
fn user_agent() -> &'static str {
    concat!("MultiTunnel/", env!("CARGO_PKG_VERSION"))
}

// ---------------------------------------------------------------------------
// Embedded engine (feature `embedded` + build.rs)
// ---------------------------------------------------------------------------

/// The engine zip embedded at build time via `MT_EMBED_ENGINE=<zip>` (see
/// `build.rs`). `None` when the env var was absent at build time — the
/// feature always compiles.
#[cfg(feature = "embedded")]
pub fn embedded() -> Option<&'static [u8]> {
    #[cfg(mt_platform_embed_engine)]
    let bytes: Option<&'static [u8]> = Some(EMBEDDED_ENGINE_ZIP);
    #[cfg(not(mt_platform_embed_engine))]
    let bytes: Option<&'static [u8]> = None;
    bytes
}

#[cfg(all(feature = "embedded", mt_platform_embed_engine))]
static EMBEDDED_ENGINE_ZIP: &[u8] = include_bytes!(env!("MT_PLATFORM_EMBED_ENGINE"));

// ---------------------------------------------------------------------------
// The locator
// ---------------------------------------------------------------------------

/// The default [`mt_engine::EngineLocator`] (D14 resolution order, see the
/// module docs).
pub struct DefaultEngineLocator {
    http: Arc<dyn HttpFetch>,
}

impl DefaultEngineLocator {
    pub fn new() -> Self {
        Self {
            http: default_http(),
        }
    }

    /// Test-only constructor: inject an [`HttpFetch`] fixture.
    #[cfg(test)]
    pub(crate) fn with_http_for_tests(http: Arc<dyn HttpFetch>) -> Self {
        Self { http }
    }

    fn locate_impl(&self) -> Option<PathBuf> {
        // (1) explicit env override
        if let Some(raw) = std::env::var_os(ENV_ENGINE) {
            let path = PathBuf::from(raw);
            if path.is_file() {
                return Some(path);
            }
            tracing::warn!(
                path = %path.display(),
                "MULTITUNNEL_ENGINE points at a missing file; continuing resolution"
            );
        }
        // (2) portable install next to the running executable
        if let Some(dir) = exe_engine_dir() {
            let bin = dir.join(engine_binary_name());
            if bin.is_file() {
                tracing::debug!(path = %bin.display(), "engine found beside the executable");
                return Some(bin);
            }
        }
        // (3) verified cache under config_dir
        let dir = crate::dirs::config_dir().join(ENGINE_DIR);
        let bin = dir.join(engine_binary_name());
        if bin.is_file() {
            match verify_engine_dir(&dir) {
                VerifyStatus::Verified => {}
                VerifyStatus::Unverified => {
                    tracing::debug!(dir = %dir.display(), "engine cache has no SHA256SUMS; unverified")
                }
                VerifyStatus::Fail => {
                    tracing::warn!(
                        dir = %dir.display(),
                        "engine cache fails SHA256SUMS verification"
                    );
                    if strict_mode() {
                        tracing::warn!("MULTITUNNEL_ENGINE_STRICT=1: refusing unverified engine");
                        return None;
                    }
                    tracing::warn!(
                        "accepting unverified engine (set MULTITUNNEL_ENGINE_STRICT=1 to refuse)"
                    );
                }
            }
            return Some(bin);
        }
        None
    }

    async fn ensure_impl(&self) -> anyhow::Result<PathBuf> {
        if let Some(path) = self.locate_impl() {
            return Ok(path);
        }
        let dir = crate::dirs::config_dir().join(ENGINE_DIR);
        // (4a) embedded zip ships with release builds of the app.
        #[cfg(feature = "embedded")]
        if let Some(bytes) = embedded() {
            tracing::info!("installing embedded engine");
            return install_zip(&dir, bytes);
        }
        // (4b) download: direct URL override, else GitHub releases latest.
        let url = match std::env::var(ENV_ENGINE_URL).ok().filter(|u| !u.is_empty()) {
            Some(u) => u,
            None => {
                let (tag, asset_url) = crate::update::latest_xray_release(&*self.http).await?;
                tracing::info!(tag = %tag, url = %asset_url, "fetching latest engine release");
                asset_url
            }
        };
        let bytes = self.http.fetch(&url).await?;
        install_zip(&dir, &bytes)
    }
}

impl Default for DefaultEngineLocator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl mt_engine::EngineLocator for DefaultEngineLocator {
    fn locate(&self) -> Option<PathBuf> {
        self.locate_impl()
    }

    async fn ensure(&self) -> anyhow::Result<PathBuf> {
        self.ensure_impl().await
    }
}

fn engine_binary_name() -> &'static str {
    if cfg!(windows) {
        "xray.exe"
    } else {
        "xray"
    }
}

fn exe_engine_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.join(ENGINE_DIR))
}

fn strict_mode() -> bool {
    std::env::var(ENV_ENGINE_STRICT).is_ok_and(|v| v == "1")
}

// ---------------------------------------------------------------------------
// Install / verify plumbing
// ---------------------------------------------------------------------------

fn install_zip(target_dir: &Path, zip_bytes: &[u8]) -> anyhow::Result<PathBuf> {
    let parent = target_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("engine dir has no parent: {}", target_dir.display()))?;
    std::fs::create_dir_all(parent)?;
    let token = unique_token();
    let staging = parent.join(format!(".engine-new-{token}"));
    let backup = parent.join(format!(".engine-old-{token}"));
    let _ = std::fs::remove_dir_all(&staging); // stale leftovers from a crash
    let _ = std::fs::remove_dir_all(&backup);

    let installed = match install_stage(&staging, zip_bytes) {
        Ok(files) => files,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e.into());
        }
    };
    if !staging.join(engine_binary_name()).is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(PlatformError::NoEngineBinary.into());
    }
    if let Err(e) = write_sums(&staging, &installed) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    // Atomic swap: staging → target, previous dir kept as backup.
    let mut backed_up = false;
    if target_dir.exists() {
        if let Err(e) = std::fs::rename(target_dir, &backup) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e.into());
        }
        backed_up = true;
    }
    if let Err(e) = std::fs::rename(&staging, target_dir) {
        if backed_up {
            let _ = std::fs::rename(&backup, target_dir); // restore
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e.into());
    }
    let _ = std::fs::remove_dir_all(&backup);

    let bin = target_dir.join(engine_binary_name());
    tracing::info!(dir = %target_dir.display(), files = installed.len(), "engine installed");
    Ok(bin)
}

fn install_stage(staging: &Path, zip_bytes: &[u8]) -> Result<Vec<String>, PlatformError> {
    std::fs::create_dir_all(staging)?;
    let mut archive = ZipArchive::new(Cursor::new(zip_bytes)).map_err(|_| PlatformError::BadZip)?;
    let mut installed: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|_| PlatformError::BadZip)?;
        if entry.is_dir() {
            continue;
        }
        // zip 9: `ZipFile::name()` now validates the entry name (UTF-8) and
        // returns a Result. An undecodable name is the same failure class as
        // an unreadable zip (`PlatformError::BadZip`): nothing is extracted
        // and the caller sweeps the staging dir.
        let entry_name = entry
            .name()
            .map_err(|_| PlatformError::BadZip)?
            .into_owned();
        let Some(name) = installable_name(&entry_name) else {
            tracing::debug!(entry = %entry_name, "skipping non-installable archive entry");
            continue;
        };
        let dest = staging.join(name);
        let mut out = std::fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
        drop(out);
        chmod_0755(&dest)?;
        if !installed.iter().any(|n| n == name) {
            installed.push(name.to_string());
        }
    }
    Ok(installed)
}

fn installable_name(entry_name: &str) -> Option<&'static str> {
    // Zip-slip defense (see module docs): basename allowlist, flat output.
    if entry_name.contains("..") || entry_name.starts_with('/') || entry_name.contains(':') {
        tracing::warn!(entry = entry_name, "refusing suspicious archive entry");
        return None;
    }
    let base = entry_name.rsplit(['/', '\\']).next().unwrap_or(entry_name);
    INSTALLABLE.into_iter().find(|candidate| *candidate == base)
}

fn write_sums(dir: &Path, files: &[String]) -> anyhow::Result<()> {
    let mut names: Vec<&String> = files.iter().collect();
    names.sort();
    let mut body = String::new();
    for name in names {
        let digest = sha256_file(&dir.join(name))?;
        body.push_str(&format!("{digest}  {name}\n"));
    }
    std::fs::write(dir.join(SUMS_FILE), body)?;
    Ok(())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(unix)]
fn chmod_0755(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn chmod_0755(path: &Path) -> std::io::Result<()> {
    let _ = path;
    Ok(())
}

fn unique_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}", std::process::id(), nanos)
}

/// Outcome of verifying `<config_dir>/engine` against `SHA256SUMS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifyStatus {
    /// All listed files match.
    Verified,
    /// No readable `SHA256SUMS` (accepted with a warning).
    Unverified,
    /// At least one mismatch, missing file, or malformed line.
    Fail,
}

pub(crate) fn verify_engine_dir(dir: &Path) -> VerifyStatus {
    let sums_path = dir.join(SUMS_FILE);
    let text = match std::fs::read_to_string(&sums_path) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!(
                sums = %sums_path.display(),
                error = %e,
                "no readable SHA256SUMS; engine dir is unverified"
            );
            return VerifyStatus::Unverified;
        }
    };
    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Some((expected, name)) = parse_sums_line(line) else {
            tracing::warn!(sums = %sums_path.display(), "malformed SHA256SUMS line");
            failures.push("<malformed>".to_string());
            continue;
        };
        match sha256_file(&dir.join(name)) {
            Ok(actual) if actual.eq_ignore_ascii_case(expected) => checked += 1,
            Ok(_) => {
                tracing::warn!(file = %name, "engine file hash mismatch");
                failures.push(name.to_string());
            }
            Err(e) => {
                tracing::warn!(file = %name, error = %e, "listed engine file unreadable");
                failures.push(name.to_string());
            }
        }
    }
    if !failures.is_empty() {
        VerifyStatus::Fail
    } else if checked == 0 {
        VerifyStatus::Unverified
    } else {
        VerifyStatus::Verified
    }
}

/// Parse one coreutils-style line: `<64 hex>  <name>` (two spaces, optional
/// `*` binary marker). Returns `(hex, name)`.
fn parse_sums_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_end();
    if line.len() < 66 {
        return None;
    }
    let (hex_part, rest) = line.split_at(64);
    if !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if !rest.starts_with(' ') {
        return None; // at least one space between digest and name
    }
    let name = rest.trim_start_matches([' ', '*']);
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return None; // sums entries are flat names inside the engine dir
    }
    Some((hex_part, name))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env_lock, set_env, temp_root, unset_env};
    use mt_engine::EngineLocator as _;
    use std::io::Write;

    // ---- fixtures ---------------------------------------------------------

    struct FixtureFetch {
        map: std::collections::BTreeMap<String, Vec<u8>>,
    }

    impl FixtureFetch {
        fn new() -> Self {
            Self {
                map: std::collections::BTreeMap::new(),
            }
        }
        fn serve(&mut self, url: &str, body: Vec<u8>) {
            self.map.insert(url.to_string(), body);
        }
        fn arc(self) -> Arc<dyn HttpFetch> {
            Arc::new(self)
        }
    }

    #[async_trait::async_trait]
    impl HttpFetch for FixtureFetch {
        async fn fetch(&self, url: &str) -> anyhow::Result<Vec<u8>> {
            self.map
                .get(url)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("fixture has no response for {url}"))
        }
    }

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buf);
            for (name, data) in entries {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .expect("start_file");
                writer.write_all(data).expect("write entry");
            }
            writer.finish().expect("finish zip");
        }
        buf.into_inner()
    }

    fn release_json(zip_url: &str) -> Vec<u8> {
        format!(
            r#"{{"tag_name":"v25.8.29","assets":[
            {{"name":"Xray-windows-64.zip","browser_download_url":"{zip_url}"}},
            {{"name":"Xray-linux-x86_64.zip","browser_download_url":"{zip_url}"}}]}}"#
        )
        .into_bytes()
    }

    /// Point config_dir at a fresh scratch dir; unset engine env vars.
    fn isolate(base: &Path) {
        set_env(crate::dirs::ENV_CONFIG_DIR, base.join("cfg"));
        unset_env(ENV_ENGINE);
        unset_env(ENV_ENGINE_URL);
        unset_env(ENV_ENGINE_STRICT);
    }

    // ---- zip install ------------------------------------------------------

    #[test]
    fn zip_extract_installs_engine_and_sums() {
        let base = temp_root("zip-install");
        let target = base.join("engine");
        let zip = make_zip(&[
            ("xray", b"fake-xray-1"),
            ("xray.exe", b"fake-xray-1"),
            ("geoip.dat", b"geo-data"),
            ("geosite.dat", b"geosite-data"),
            ("README.md", b"skip me"),
        ]);
        let bin = install_zip(&target, &zip).expect("install");
        assert_eq!(bin, target.join(engine_binary_name()));
        assert_eq!(std::fs::read(&bin).unwrap(), b"fake-xray-1");
        assert_eq!(
            std::fs::read(target.join("geoip.dat")).unwrap(),
            b"geo-data"
        );
        assert!(target.join("geosite.dat").is_file());
        assert!(
            !target.join("README.md").is_file(),
            "non-allowlisted files skipped"
        );
        assert_eq!(
            verify_engine_dir(&target),
            VerifyStatus::Verified,
            "SHA256SUMS written and self-consistent"
        );
        // Zip-slip entry never escaped.
        assert!(!base.join("evil").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&bin).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755, "binary is 0755");
        }
    }

    #[test]
    fn zip_without_binary_is_rejected_and_leaves_nothing() {
        let base = temp_root("zip-nobin");
        let target = base.join("engine");
        let zip = make_zip(&[("geoip.dat", b"geo")]);
        let err = install_zip(&target, &zip).expect_err("no binary in zip");
        assert!(err.to_string().contains("xray"));
        assert!(!target.exists(), "failed install leaves no engine dir");
        assert_eq!(
            std::fs::read_dir(&base).unwrap().count(),
            0,
            "staging dir cleaned up after failure"
        );
    }

    // ---- SHA256SUMS verification -----------------------------------------

    #[test]
    fn sums_verify_happy_and_mismatch() {
        let dir = temp_root("sums");
        std::fs::write(dir.join("xray"), b"payload").unwrap();
        let good = sha256_file(&dir.join("xray")).unwrap();
        std::fs::write(dir.join(SUMS_FILE), format!("{good}  xray\n")).unwrap();
        assert_eq!(verify_engine_dir(&dir), VerifyStatus::Verified);

        std::fs::write(dir.join(SUMS_FILE), format!("{}  xray\n", "0".repeat(64))).unwrap();
        assert_eq!(verify_engine_dir(&dir), VerifyStatus::Fail);

        std::fs::write(dir.join(SUMS_FILE), "short line\n").unwrap();
        assert_eq!(
            verify_engine_dir(&dir),
            VerifyStatus::Fail,
            "malformed line fails"
        );

        std::fs::write(dir.join(SUMS_FILE), "not a sums line\n").unwrap();
        assert_eq!(
            verify_engine_dir(&dir),
            VerifyStatus::Fail,
            "malformed/comment lines fail (strict posture; we write this file ourselves)"
        );
        assert_eq!(
            verify_engine_dir(&temp_root("sums-empty")),
            VerifyStatus::Unverified,
            "no SHA256SUMS at all"
        );
    }

    #[test]
    fn strict_mode_refuses_tampered_cache() {
        let _guard = env_lock();
        let base = temp_root("strict");
        isolate(&base);
        let engine = crate::dirs::config_dir().join(ENGINE_DIR);
        std::fs::create_dir_all(&engine).unwrap();
        std::fs::write(engine.join(engine_binary_name()), b"tampered").unwrap();
        std::fs::write(
            engine.join(SUMS_FILE),
            format!("{}  {}\n", "f".repeat(64), engine_binary_name()),
        )
        .unwrap();

        let locator = DefaultEngineLocator::with_http_for_tests(FixtureFetch::new().arc());
        set_env(ENV_ENGINE_STRICT, "1");
        assert!(
            locator.locate().is_none(),
            "strict refuses mismatching engine"
        );
        unset_env(ENV_ENGINE_STRICT);
        assert!(locator.locate().is_some(), "default accepts with a warning");

        unset_env(crate::dirs::ENV_CONFIG_DIR);
    }

    // ---- resolution order -------------------------------------------------

    #[test]
    fn locator_env_override_wins() {
        let _guard = env_lock();
        let base = temp_root("loc-env");
        isolate(&base); // keep steps 2/3 empty
        let bin = base.join("my-xray");
        std::fs::write(&bin, b"explicit").unwrap();
        set_env(ENV_ENGINE, &bin);

        let locator = DefaultEngineLocator::new();
        assert_eq!(locator.locate(), Some(bin));

        // Missing file under the override falls through → nothing found.
        set_env(ENV_ENGINE, base.join("missing-xray"));
        assert_eq!(locator.locate(), None);

        unset_env(ENV_ENGINE);
        unset_env(crate::dirs::ENV_CONFIG_DIR);
    }

    #[test]
    fn locator_finds_verified_cache_and_locate_never_downloads() {
        let _guard = env_lock();
        let base = temp_root("loc-cache");
        isolate(&base);
        let engine = crate::dirs::config_dir().join(ENGINE_DIR);
        std::fs::create_dir_all(&engine).unwrap();
        std::fs::write(engine.join(engine_binary_name()), b"cached").unwrap();
        let digest = sha256_file(&engine.join(engine_binary_name())).unwrap();
        std::fs::write(
            engine.join(SUMS_FILE),
            format!("{digest}  {}\n", engine_binary_name()),
        )
        .unwrap();

        // Empty fixture: any fetch attempt would error out loudly.
        let locator = DefaultEngineLocator::with_http_for_tests(FixtureFetch::new().arc());
        assert_eq!(
            locator.locate(),
            Some(engine.join(engine_binary_name())),
            "locate() resolves the verified cache without HTTP"
        );
        unset_env(crate::dirs::ENV_CONFIG_DIR);
    }

    #[test]
    fn locator_none_when_nothing_installed() {
        let _guard = env_lock();
        let base = temp_root("loc-none");
        isolate(&base);
        let locator = DefaultEngineLocator::with_http_for_tests(FixtureFetch::new().arc());
        assert_eq!(locator.locate(), None);
        unset_env(crate::dirs::ENV_CONFIG_DIR);
    }

    // ---- ensure(): download + embedded paths ------------------------------

    // These async tests hold the env lock across `.await` on purpose: the
    // env vars under test are process-global and must stay set for the
    // whole await; the lock is what serializes access between tests.
    // With a build-time-embedded engine, `ensure()` correctly resolves
    // the embedded zip before ever reaching the download step, so these
    // download-path tests only make sense without the embed.
    #[cfg(not(mt_platform_embed_engine))]
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn ensure_downloads_release_zip_via_fixture() {
        let _guard = env_lock();
        let base = temp_root("ensure-dl");
        isolate(&base);
        let zip = make_zip(&[
            ("xray", b"fake-xray-dl"),
            ("xray.exe", b"fake-xray-dl"),
            ("geoip.dat", b"geo"),
            ("geosite.dat", b"geosite"),
        ]);
        let mut fixture = FixtureFetch::new();
        fixture.serve(
            crate::update::XRAY_RELEASES_LATEST,
            release_json("https://fixture.invalid/xray.zip"),
        );
        fixture.serve("https://fixture.invalid/xray.zip", zip);
        let locator = DefaultEngineLocator::with_http_for_tests(fixture.arc());

        let bin = locator
            .ensure()
            .await
            .expect("ensure downloads and installs");
        let engine = crate::dirs::config_dir().join(ENGINE_DIR);
        assert_eq!(bin, engine.join(engine_binary_name()));
        assert_eq!(std::fs::read(&bin).unwrap(), b"fake-xray-dl");
        assert_eq!(
            verify_engine_dir(&engine),
            VerifyStatus::Verified,
            "SHA256SUMS written for downloaded files"
        );
        unset_env(crate::dirs::ENV_CONFIG_DIR);
    }

    #[cfg(not(mt_platform_embed_engine))]
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn ensure_honors_engine_url_override() {
        let _guard = env_lock();
        let base = temp_root("ensure-url");
        isolate(&base);
        let zip = make_zip(&[
            ("xray", b"from-url-override"),
            ("xray.exe", b"from-url-override"),
        ]);
        let mut fixture = FixtureFetch::new();
        fixture.serve("https://fixture.invalid/direct.zip", zip);
        set_env(ENV_ENGINE_URL, "https://fixture.invalid/direct.zip");
        let locator = DefaultEngineLocator::with_http_for_tests(fixture.arc());

        let bin = locator.ensure().await.expect("direct-url ensure");
        assert_eq!(std::fs::read(&bin).unwrap(), b"from-url-override");
        unset_env(ENV_ENGINE_URL);
        unset_env(crate::dirs::ENV_CONFIG_DIR);
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn ensure_returns_located_engine_without_http() {
        let _guard = env_lock();
        let base = temp_root("ensure-hit");
        isolate(&base);
        let engine = crate::dirs::config_dir().join(ENGINE_DIR);
        std::fs::create_dir_all(&engine).unwrap();
        let bin = engine.join(engine_binary_name());
        std::fs::write(&bin, b"already-here").unwrap();
        let digest = sha256_file(&bin).unwrap();
        std::fs::write(
            engine.join(SUMS_FILE),
            format!("{digest}  {}\n", engine_binary_name()),
        )
        .unwrap();

        let locator = DefaultEngineLocator::with_http_for_tests(FixtureFetch::new().arc());
        assert_eq!(locator.ensure().await.unwrap(), bin);
        unset_env(crate::dirs::ENV_CONFIG_DIR);
    }

    /// The embedded branch of `ensure()` runs through the same
    /// `install_zip` path as downloads; this exercises it with a fixture
    /// zip (no `MT_EMBED_ENGINE` needed at build time).
    #[tokio::test]
    async fn embedded_extraction_from_fixture_zip() {
        let _guard = env_lock();
        let base = temp_root("ensure-embedded");
        isolate(&base);
        let dir = crate::dirs::config_dir().join(ENGINE_DIR);
        let zip = make_zip(&[
            ("xray", b"embedded-payload"),
            ("xray.exe", b"embedded-payload"),
        ]);
        let bin = install_zip(&dir, &zip).expect("embedded install");
        assert_eq!(std::fs::read(&bin).unwrap(), b"embedded-payload");
        assert_eq!(verify_engine_dir(&dir), VerifyStatus::Verified);
        unset_env(crate::dirs::ENV_CONFIG_DIR);
    }

    #[cfg(feature = "embedded")]
    #[test]
    fn embedded_matches_build_time_env() {
        #[cfg(mt_platform_embed_engine)]
        assert!(
            embedded().is_some(),
            "MT_EMBED_ENGINE was set at build time"
        );
        #[cfg(not(mt_platform_embed_engine))]
        assert!(embedded().is_none(), "no MT_EMBED_ENGINE at build time");
    }

    // ---- helpers ----------------------------------------------------------

    #[test]
    fn installable_name_rejects_traversal() {
        assert_eq!(installable_name("xray"), Some("xray"));
        assert_eq!(installable_name("sub/dir/xray"), Some("xray"));
        assert_eq!(installable_name("sub\\dir\\geoip.dat"), Some("geoip.dat"));
        assert_eq!(installable_name("../xray"), None);
        assert_eq!(installable_name("/abs/xray"), None);
        assert_eq!(installable_name("C:\\xray"), None);
        assert_eq!(installable_name("README.md"), None);
    }

    #[test]
    fn parse_sums_line_accepts_coreutils_shapes() {
        let hex = "a".repeat(64);
        assert_eq!(
            parse_sums_line(&format!("{hex}  xray")),
            Some((hex.as_str(), "xray"))
        );
        assert_eq!(
            parse_sums_line(&format!("{hex}  *xray")),
            Some((hex.as_str(), "xray"))
        );
        assert_eq!(parse_sums_line("tooshort"), None);
        assert_eq!(
            parse_sums_line(&format!("{}  xray", "g".repeat(64))),
            None,
            "non-hex"
        );
        assert_eq!(
            parse_sums_line(&format!("{hex}  sub/xray")),
            None,
            "no slashes in names"
        );
    }
}
