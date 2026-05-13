use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use reqwest::header::USER_AGENT;
use serde::Deserialize;

pub const INSTALL_METHOD_ENV: &str = "AGENTS_NOTIFIER_INSTALL_METHOD";
pub const INSTALL_METHOD_MARKER_FILE: &str = ".agents-notifier-install-method";
pub const SKIP_UPDATE_CHECK_ENV: &str = "AGENTS_NOTIFIER_SKIP_UPDATE_CHECK";

const NPM_PACKAGE_URL: &str = "https://registry.npmjs.org/agents-notifier";
const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/lumpinif/agents-notifier/releases/latest";
const UPDATE_CHECK_USER_AGENT: &str = concat!("agents-notifier/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AppVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

impl AppVersion {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        let parts = value.split('.').collect::<Vec<_>>();
        if parts.len() != 3 {
            anyhow::bail!("version must look like `x.y.z`; got `{value}`");
        }

        let major = parse_version_part(parts[0], value)?;
        let minor = parse_version_part(parts[1], value)?;
        let patch = parse_version_part(parts[2], value)?;

        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl fmt::Display for AppVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateInstallMethod {
    Npx,
    Npm,
    Script,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatestVersionSource {
    NpmRegistry,
    GitHubRelease,
}

impl UpdateInstallMethod {
    pub fn latest_version_source(self) -> LatestVersionSource {
        match self {
            Self::Npx | Self::Npm => LatestVersionSource::NpmRegistry,
            Self::Script => LatestVersionSource::GitHubRelease,
        }
    }
}

#[derive(Debug, Deserialize)]
struct NpmPackageMetadata {
    #[serde(rename = "dist-tags")]
    dist_tags: NpmDistTags,
}

#[derive(Debug, Deserialize)]
struct NpmDistTags {
    latest: String,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseMetadata {
    tag_name: String,
}

pub fn should_skip_update_check() -> bool {
    skip_update_check_enabled(std::env::var(SKIP_UPDATE_CHECK_ENV).ok().as_deref())
}

pub fn resolve_update_install_method(
    current_exe: &Path,
    install_method: Option<&str>,
    install_method_marker: Option<&str>,
    script_binary_path: Option<&Path>,
) -> Option<UpdateInstallMethod> {
    if is_development_binary(current_exe) {
        return None;
    }

    if let Some(method) = install_method.and_then(parse_install_method) {
        return Some(method);
    }

    if let Some(method) = install_method_marker.and_then(parse_install_method) {
        return Some(method);
    }

    if script_binary_path.is_some_and(|script_binary_path| script_binary_path == current_exe) {
        return Some(UpdateInstallMethod::Script);
    }

    None
}

pub fn install_method_marker_path(binary_path: &Path) -> Option<PathBuf> {
    binary_path
        .parent()
        .map(|parent| parent.join(INSTALL_METHOD_MARKER_FILE))
}

pub fn read_install_method_marker(binary_path: &Path) -> anyhow::Result<Option<String>> {
    let Some(path) = install_method_marker_path(binary_path) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read install method marker `{}`", path.display()))?;
    let value = raw.trim();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value.to_string()))
    }
}

pub fn parse_npm_latest_version(raw: &str) -> anyhow::Result<AppVersion> {
    let metadata: NpmPackageMetadata =
        serde_json::from_str(raw).context("failed to parse npm package metadata")?;
    AppVersion::parse(&metadata.dist_tags.latest)
}

pub fn parse_github_latest_version(raw: &str) -> anyhow::Result<AppVersion> {
    let metadata: GitHubReleaseMetadata =
        serde_json::from_str(raw).context("failed to parse GitHub release metadata")?;
    let version = metadata
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&metadata.tag_name);
    AppVersion::parse(version)
}

pub fn update_is_available(current: AppVersion, latest: AppVersion) -> bool {
    latest > current
}

pub async fn fetch_latest_version(source: LatestVersionSource) -> anyhow::Result<AppVersion> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("failed to build update check HTTP client")?;

    let url = match source {
        LatestVersionSource::NpmRegistry => NPM_PACKAGE_URL,
        LatestVersionSource::GitHubRelease => GITHUB_LATEST_RELEASE_URL,
    };

    let raw = client
        .get(url)
        .header(USER_AGENT, UPDATE_CHECK_USER_AGENT)
        .send()
        .await
        .with_context(|| format!("failed to request latest version from `{url}`"))?
        .error_for_status()
        .with_context(|| format!("latest version request failed for `{url}`"))?
        .text()
        .await
        .with_context(|| format!("failed to read latest version response from `{url}`"))?;

    match source {
        LatestVersionSource::NpmRegistry => parse_npm_latest_version(&raw),
        LatestVersionSource::GitHubRelease => parse_github_latest_version(&raw),
    }
}

fn parse_version_part(part: &str, full_version: &str) -> anyhow::Result<u64> {
    if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        anyhow::bail!("version must look like `x.y.z`; got `{full_version}`");
    }

    part.parse::<u64>()
        .with_context(|| format!("failed to parse version `{full_version}`"))
}

fn is_development_binary(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "target")
}

fn skip_update_check_enabled(value: Option<&str>) -> bool {
    value.map(|value| value.trim() == "1").unwrap_or(false)
}

fn parse_install_method(value: &str) -> Option<UpdateInstallMethod> {
    match value.trim() {
        "npx" => Some(UpdateInstallMethod::Npx),
        "npm" => Some(UpdateInstallMethod::Npm),
        "script" => Some(UpdateInstallMethod::Script),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn parses_strict_three_part_versions() {
        assert_eq!(AppVersion::parse("0.7.4").unwrap().to_string(), "0.7.4");

        assert!(AppVersion::parse("0.7").is_err());
        assert!(AppVersion::parse("v0.7.4").is_err());
        assert!(AppVersion::parse("0.7.4-beta").is_err());
        assert!(AppVersion::parse("0.7.x").is_err());
    }

    #[test]
    fn compares_versions() {
        let current = AppVersion::parse("0.7.4").unwrap();

        assert!(update_is_available(
            current,
            AppVersion::parse("0.8.0").unwrap()
        ));
        assert!(!update_is_available(
            current,
            AppVersion::parse("0.7.4").unwrap()
        ));
        assert!(!update_is_available(
            current,
            AppVersion::parse("0.7.3").unwrap()
        ));
    }

    #[test]
    fn parses_npm_latest_version() {
        let raw = r#"{"dist-tags":{"latest":"0.8.0"}}"#;

        let version = parse_npm_latest_version(raw).unwrap();

        assert_eq!(version.to_string(), "0.8.0");
    }

    #[test]
    fn parses_github_latest_version() {
        let raw = r#"{"tag_name":"v0.8.0"}"#;

        let version = parse_github_latest_version(raw).unwrap();

        assert_eq!(version.to_string(), "0.8.0");
    }

    #[test]
    fn skips_development_binary_paths() {
        assert_eq!(
            resolve_update_install_method(
                Path::new("/repo/target/debug/agents-notifier"),
                None,
                None,
                None
            ),
            None
        );
        assert_eq!(
            resolve_update_install_method(
                Path::new("/Users/tester/.local/bin/agents-notifier"),
                None,
                None,
                Some(Path::new("/Users/tester/.local/bin/agents-notifier"))
            ),
            Some(UpdateInstallMethod::Script)
        );
    }

    #[test]
    fn detects_skip_update_check_env_value() {
        assert!(skip_update_check_enabled(Some("1")));
        assert!(!skip_update_check_enabled(Some("true")));
        assert!(!skip_update_check_enabled(Some("0")));
        assert!(!skip_update_check_enabled(Some("")));
        assert!(!skip_update_check_enabled(Some("   ")));
        assert!(!skip_update_check_enabled(None));
    }

    #[test]
    fn selects_install_method_from_environment() {
        let binary = Path::new("/Users/tester/.local/bin/agents-notifier");

        assert_eq!(
            resolve_update_install_method(binary, Some("npx"), None, None),
            Some(UpdateInstallMethod::Npx)
        );
        assert_eq!(
            resolve_update_install_method(binary, Some("npm"), None, None),
            Some(UpdateInstallMethod::Npm)
        );
        assert_eq!(
            resolve_update_install_method(binary, None, Some("script"), None),
            Some(UpdateInstallMethod::Script)
        );
    }

    #[test]
    fn unknown_install_method_without_known_script_path_skips_update() {
        let binary = Path::new("/Users/tester/.cargo/bin/agents-notifier");

        assert_eq!(
            resolve_update_install_method(binary, Some("cargo"), None, None),
            None
        );
        assert_eq!(
            resolve_update_install_method(binary, None, None, None),
            None
        );
    }

    #[test]
    fn selects_latest_source_by_install_method() {
        assert_eq!(
            UpdateInstallMethod::Npx.latest_version_source(),
            LatestVersionSource::NpmRegistry
        );
        assert_eq!(
            UpdateInstallMethod::Npm.latest_version_source(),
            LatestVersionSource::NpmRegistry
        );
        assert_eq!(
            UpdateInstallMethod::Script.latest_version_source(),
            LatestVersionSource::GitHubRelease
        );
    }
}
