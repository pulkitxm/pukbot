use std::fs;
use std::io::Write;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const API_URL: &str = "https://api.github.com/repos/pulkitxm/pukbot/releases/latest";
const RELEASES_URL: &str = "https://github.com/pulkitxm/pukbot/releases";
const METADATA_LIMIT: usize = 1024 * 1024;
const CHECKSUM_LIMIT: usize = 64 * 1024;
const BINARY_LIMIT: usize = 64 * 1024 * 1024;
const USER_AGENT: &str = concat!("pukbot/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    UpToDate,
    Available,
    Updated,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    pub status: UpdateStatus,
    pub current_version: String,
    pub latest_version: String,
    pub asset: Option<String>,
}

impl UpdateResult {
    pub fn text(&self) -> String {
        match self.status {
            UpdateStatus::UpToDate => format!("Pukbot {} is up to date", self.current_version),
            UpdateStatus::Available => format!(
                "Pukbot {} is available, current version is {}",
                self.latest_version, self.current_version
            ),
            UpdateStatus::Updated => format!(
                "Updated Pukbot from {} to {}",
                self.current_version, self.latest_version
            ),
        }
    }
}

#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
}

pub fn run(check_only: bool) -> Result<UpdateResult> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .context("compiled version is not semantic versioning")?;
    let release = parse_release(&fetch(API_URL, METADATA_LIMIT)?)?;
    if release.1 <= current {
        return Ok(UpdateResult {
            status: UpdateStatus::UpToDate,
            current_version: current.to_string(),
            latest_version: release.1.to_string(),
            asset: None,
        });
    }
    let asset = asset_for(std::env::consts::OS, std::env::consts::ARCH)?;
    if check_only {
        return Ok(UpdateResult {
            status: UpdateStatus::Available,
            current_version: current.to_string(),
            latest_version: release.1.to_string(),
            asset: Some(asset.to_owned()),
        });
    }
    let release_url = format!("{RELEASES_URL}/download/{}", release.0);
    let checksum_bytes = fetch(&format!("{release_url}/SHA256SUMS"), CHECKSUM_LIMIT)?;
    let checksums = std::str::from_utf8(&checksum_bytes)
        .context("release checksum file was not valid UTF-8")?;
    let expected = release_checksum(checksums, asset)?;
    let binary = fetch(&format!("{release_url}/{asset}"), BINARY_LIMIT)?;
    ensure!(sha256(&binary) == expected, "checksum verification failed");
    let mut staged = tempfile::NamedTempFile::new().context("failed to stage update")?;
    staged
        .write_all(&binary)
        .context("failed to stage update")?;
    staged.flush().context("failed to flush staged update")?;
    self_replace::self_replace(staged.path()).context("failed to replace Pukbot executable")?;
    Ok(UpdateResult {
        status: UpdateStatus::Updated,
        current_version: current.to_string(),
        latest_version: release.1.to_string(),
        asset: Some(asset.to_owned()),
    })
}

fn fetch(url: &str, limit: usize) -> Result<Vec<u8>> {
    let destination = tempfile::NamedTempFile::new()
        .context("failed to create update download")?
        .into_temp_path();
    let output = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "30",
            "--max-filesize",
            &limit.to_string(),
            "--user-agent",
            USER_AGENT,
            "--output",
        ])
        .arg(destination.as_os_str())
        .arg(url)
        .output()
        .context("failed to launch curl for update")?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        bail!("update download failed: {}", message.trim());
    }
    let bytes = fs::read(&destination).context("failed to read update download")?;
    ensure!(bytes.len() <= limit, "update download exceeded size limit");
    Ok(bytes)
}

fn parse_release(bytes: &[u8]) -> Result<(String, Version)> {
    let release = serde_json::from_slice::<LatestRelease>(bytes)
        .context("failed to decode latest release")?;
    let version = release
        .tag_name
        .strip_prefix('v')
        .context("latest release tag must start with v")?;
    let version = Version::parse(version).context("latest release tag is not stable semver")?;
    ensure!(version.pre.is_empty(), "latest release is not stable");
    Ok((release.tag_name, version))
}

fn release_checksum(document: &str, asset: &str) -> Result<String> {
    let matches = document
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let checksum = fields.next()?;
            let name = fields
                .next()?
                .trim_start_matches('*')
                .trim_start_matches("dist/");
            (name == asset).then_some(checksum)
        })
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "release checksum entry is missing or duplicated"
    );
    let checksum = matches[0];
    ensure!(
        checksum.len() == 64
            && checksum
                .chars()
                .all(|character| character.is_ascii_hexdigit()),
        "release checksum is invalid"
    );
    Ok(checksum.to_ascii_lowercase())
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn asset_for(os: &str, arch: &str) -> Result<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Ok("pukbot-linux-x86_64"),
        ("linux", "aarch64") => Ok("pukbot-linux-aarch64"),
        ("macos", "x86_64") => Ok("pukbot-macos-x86_64"),
        ("macos", "aarch64") => Ok("pukbot-macos-aarch64"),
        ("windows", "x86_64" | "aarch64") => Ok("pukbot-windows-x86_64.exe"),
        _ => bail!("Pukbot does not publish an update for {os} {arch}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{asset_for, parse_release, release_checksum};

    #[test]
    fn parses_stable_release() {
        let release =
            parse_release(br#"{"tag_name":"v1.2.3"}"#).expect("stable release should parse");
        assert_eq!(release.0, "v1.2.3");
        assert_eq!(release.1.to_string(), "1.2.3");
    }

    #[test]
    fn selects_platform_assets() {
        assert_eq!(
            asset_for("macos", "aarch64").expect("asset should exist"),
            "pukbot-macos-aarch64"
        );
        assert!(asset_for("freebsd", "x86_64").is_err());
    }

    #[test]
    fn reads_exact_checksum() {
        let checksum = "a".repeat(64);
        let document = format!("{checksum}  pukbot-linux-x86_64\n");
        assert_eq!(
            release_checksum(&document, "pukbot-linux-x86_64").expect("checksum should parse"),
            checksum
        );
    }
}
