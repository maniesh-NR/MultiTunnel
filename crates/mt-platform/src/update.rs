//! Read-only update check against the Xray-core GitHub releases.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert D"):
//! `latest_release() -> anyhow::Result<Option<String>>` queries the GitHub
//! API `releases/latest` endpoint (`api.github.com`, repo `XTLS/
//! Xray-core`), compares the returned tag against the crate version and
//! returns `Some(tag)` when a newer release exists, `None` otherwise.
//! NO auto-apply, NO secrets sent, HTTP behind the `net` feature like the
//! rest of the crate.
//!
//! IMPLEMENTATION NOTES (picked and documented):
//! * Tag comparison: date-based Xray tags (`v25.8.29`) are compared
//!   numerically component-by-component against the current version
//!   (`v`/`V` prefixes stripped, missing components count as 0), so
//!   `v25.8.29 > 0.1.0`. If either side does not parse as dotted numbers,
//!   any tag difference is reported (informational only).
//! * `latest_release()` compares against THIS crate's `CARGO_PKG_VERSION`
//!   (the frozen skeleton's doc: "tag compare vs CARGO_PKG_VERSION");
//!   [`latest_release_for`] lets the orchestrator pass the app version
//!   explicitly when the shipped engine tag policy differs.
//! * The engine asset lookup (`latest_xray_release`) used by the download
//!   path shares the same API response types.

use serde::Deserialize;

use crate::engine::{default_http, HttpFetch, PlatformError};

/// GitHub releases/latest for the managed engine.
pub(crate) const XRAY_RELEASES_LATEST: &str =
    "https://api.github.com/repos/XTLS/Xray-core/releases/latest";

#[derive(Debug, Deserialize)]
pub(crate) struct GhRelease {
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
}

/// Latest release tag, if a newer one exists. Read-only; no auto-apply.
pub async fn latest_release() -> anyhow::Result<Option<String>> {
    latest_release_for(env!("CARGO_PKG_VERSION")).await
}

/// [`latest_release`] compared against an explicit current version.
pub async fn latest_release_for(current: &str) -> anyhow::Result<Option<String>> {
    let http = default_http();
    let release = fetch_release(&*http).await?;
    Ok(if tag_is_newer(&release.tag_name, current) {
        Some(release.tag_name)
    } else {
        None
    })
}

pub(crate) async fn fetch_release(http: &dyn HttpFetch) -> anyhow::Result<GhRelease> {
    let bytes = http.fetch(XRAY_RELEASES_LATEST).await?;
    serde_json::from_slice(&bytes).map_err(|_| PlatformError::BadRelease.into())
}

/// `(tag, browser_download_url)` of the engine zip asset for this OS.
pub(crate) async fn latest_xray_release(http: &dyn HttpFetch) -> anyhow::Result<(String, String)> {
    let release = fetch_release(http).await?;
    let want = engine_asset_name();
    let url = release
        .assets
        .iter()
        .find(|asset| asset.name == want)
        .map(|asset| asset.browser_download_url.clone())
        .ok_or_else(|| PlatformError::NoAsset {
            tag: release.tag_name.clone(),
            asset: want.to_string(),
        })?;
    Ok((release.tag_name, url))
}

/// The Xray release zip asset name for the target OS (contract literals).
pub(crate) fn engine_asset_name() -> &'static str {
    if cfg!(windows) {
        "Xray-windows-64.zip"
    } else {
        "Xray-linux-x86_64.zip"
    }
}

/// Numeric component-wise comparison (missing components = 0); falls back
/// to "different means newer" when a tag is not dotted-numeric.
pub(crate) fn tag_is_newer(tag: &str, current: &str) -> bool {
    match (parse_version(strip_v(tag)), parse_version(strip_v(current))) {
        (Some(tag_ver), Some(cur_ver)) => ver_gt(&tag_ver, &cur_ver),
        _ => tag != current,
    }
}

fn strip_v(s: &str) -> &str {
    s.strip_prefix('v')
        .or_else(|| s.strip_prefix('V'))
        .unwrap_or(s)
}

fn parse_version(s: &str) -> Option<Vec<u64>> {
    let parts: Option<Vec<u64>> = s.split('.').map(|p| p.parse().ok()).collect();
    match parts {
        Some(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn ver_gt(a: &[u64], b: &[u64]) -> bool {
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_compare_pure_logic() {
        assert!(tag_is_newer("v25.8.29", "0.1.0"));
        assert!(tag_is_newer("25.8.29", "25.8.28"));
        assert!(tag_is_newer("v1.0", "v0.9.9"));
        assert!(!tag_is_newer("v0.1.0", "0.1.0"), "equal → None");
        assert!(!tag_is_newer("v0.0.9", "0.1.0"), "older → None");
        assert!(!tag_is_newer("v0.1.0", "0.1"), "padded equal → None");
        assert!(tag_is_newer("v0.2", "0.1.9"));
        assert!(
            tag_is_newer("some-beta", "0.1.0"),
            "unparseable + different → Some"
        );
        assert!(!tag_is_newer("same", "same"), "unparseable + equal → None");
    }

    #[tokio::test]
    async fn latest_release_reports_newer_tag_via_fixture() {
        let json = br#"{"tag_name":"v25.8.29","assets":[
            {"name":"Xray-linux-x86_64.zip","browser_download_url":"https://fixture.invalid/x.zip"}
        ]}"#;
        let http = crate::test_support::fixture_http(json.to_vec());
        let release = fetch_release(http.as_ref()).await.expect("fixture release");
        assert_eq!(release.tag_name, "v25.8.29");
        assert_eq!(release.assets.len(), 1);
        assert!(tag_is_newer(&release.tag_name, "0.1.0"));

        let same = br#"{"tag_name":"v0.1.0","assets":[]}"#;
        let http_same = crate::test_support::fixture_http(same.to_vec());
        let release_same = fetch_release(http_same.as_ref())
            .await
            .expect("fixture release");
        assert!(!tag_is_newer(&release_same.tag_name, "0.1.0"));
    }

    #[tokio::test]
    async fn latest_xray_release_picks_os_asset() {
        let json = br#"{"tag_name":"v9.9.9","assets":[
            {"name":"README.txt","browser_download_url":"https://fixture.invalid/readme"},
            {"name":"Xray-linux-x86_64.zip","browser_download_url":"https://fixture.invalid/linux.zip"},
            {"name":"Xray-windows-64.zip","browser_download_url":"https://fixture.invalid/win.zip"}
        ]}"#;
        let http = crate::test_support::fixture_http(json.to_vec());
        let (tag, url) = latest_xray_release(http.as_ref())
            .await
            .expect("asset lookup");
        assert_eq!(tag, "v9.9.9");
        let want = if cfg!(windows) {
            "https://fixture.invalid/win.zip"
        } else {
            "https://fixture.invalid/linux.zip"
        };
        assert_eq!(url, want, "engine_asset_name() = {:?}", engine_asset_name());
    }

    #[tokio::test]
    async fn missing_asset_is_a_structured_error() {
        let json = br#"{"tag_name":"v9.9.9","assets":[]}"#;
        let http = crate::test_support::fixture_http(json.to_vec());
        let err = latest_xray_release(http.as_ref())
            .await
            .expect_err("no assets at all");
        assert!(
            err.to_string().contains("Xray-"),
            "error names the wanted asset"
        );
    }
}
