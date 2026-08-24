use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const WORKFLOW_REPOSITORY: &str = "pulkitxm/pukbot";
const ASSET_RELEASE: &str = "comment-assets";
const WORKFLOW_REF: &str = "main";
const MAX_MEDIA_BYTES: u64 = 100 * 1024 * 1024;
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "avif", "svg", "mp4", "mov", "webm",
    "mkv", "m4v", "mp3", "wav", "ogg", "m4a", "flac", "aac", "pdf", "txt", "md", "json", "csv",
    "zip", "gz", "tgz", "tar", "7z",
];
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "avif", "svg",
];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "webm", "mkv", "m4v"];
const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "m4a", "flac", "aac"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaInput {
    pub name: String,
    pub path: Option<PathBuf>,
    pub url: Option<String>,
    pub alt: Option<String>,
}

pub fn supported_extensions() -> Vec<&'static str> {
    SUPPORTED_EXTENSIONS.to_vec()
}

pub fn resolve(input: &MediaInput, dry_run: bool) -> Result<String> {
    validate_name(&input.name)?;
    let label = input.alt.as_deref().unwrap_or(&input.name);
    let (url, extension) = match (&input.path, &input.url) {
        (Some(path), None) => prepare_local(path, dry_run)?,
        (None, Some(url)) => prepare_url(url)?,
        _ => bail!(
            "media {} must provide exactly one of path or url",
            input.name
        ),
    };
    Ok(markdown(label, &url, &extension))
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        bail!("media names may contain only ASCII letters, numbers, underscores, and hyphens");
    }
    Ok(())
}

fn prepare_url(url: &str) -> Result<(String, String)> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        bail!("media URL must use HTTP or HTTPS");
    }
    if url.contains(['\n', '\r', ')']) {
        bail!("media URL contains unsupported characters");
    }
    let extension = url
        .split('?')
        .next()
        .and_then(|value| Path::new(value).extension())
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    Ok((url.to_owned(), extension))
}

fn prepare_local(path: &Path, dry_run: bool) -> Result<(String, String)> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read local media {}", path.display()))?;
    if !metadata.is_file() {
        bail!("local media is not a regular file: {}", path.display());
    }
    if metadata.len() == 0 || metadata.len() > MAX_MEDIA_BYTES {
        bail!("local media must be between 1 byte and {MAX_MEDIA_BYTES} bytes");
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .context("local media must have a supported file extension")?;
    if !SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
        bail!("unsupported local media extension: {extension}");
    }
    validate_signature(path, &extension)?;
    let digest = Sha256::digest(
        fs::read(path).with_context(|| format!("failed to read local media {}", path.display()))?,
    );
    let asset_name = format!("pukbot-{}.{}", hex::encode(digest), extension);
    let url = format!(
        "https://github.com/{WORKFLOW_REPOSITORY}/releases/download/{ASSET_RELEASE}/{asset_name}"
    );
    if dry_run {
        return Ok((url, extension));
    }
    ensure_asset_release()?;
    let directory = tempfile::tempdir().context("failed to prepare local media upload")?;
    let staged_path = directory.path().join(&asset_name);
    fs::copy(path, &staged_path).context("failed to stage local media upload")?;
    let status = Command::new("gh")
        .args(["release", "upload", ASSET_RELEASE])
        .arg(&staged_path)
        .args(["--repo", WORKFLOW_REPOSITORY, "--clobber"])
        .status()
        .context("failed to launch gh for the local media upload")?;
    if !status.success() {
        bail!("failed to upload local media to GitHub Releases");
    }
    Ok((url, extension))
}

fn validate_signature(path: &Path, extension: &str) -> Result<()> {
    if matches!(extension, "txt" | "md" | "json" | "csv" | "svg") {
        let bytes = fs::read(path)
            .with_context(|| format!("failed to inspect local media {}", path.display()))?;
        std::str::from_utf8(&bytes).context("text media must be valid UTF-8")?;
        return Ok(());
    }
    infer::get_from_path(path)
        .with_context(|| format!("failed to inspect local media {}", path.display()))?
        .context("local media signature was not recognized")?;
    Ok(())
}

fn markdown(label: &str, url: &str, extension: &str) -> String {
    let safe_label = label.replace(['[', ']'], "");
    if IMAGE_EXTENSIONS.contains(&extension) {
        format!("![{safe_label}]({url})")
    } else if VIDEO_EXTENSIONS.contains(&extension) {
        format!("[{safe_label} (video)]({url})")
    } else if AUDIO_EXTENSIONS.contains(&extension) {
        format!("[{safe_label} (audio)]({url})")
    } else {
        format!("[{safe_label}]({url})")
    }
}

fn ensure_asset_release() -> Result<()> {
    if asset_release_exists()? {
        return Ok(());
    }
    let status = Command::new("gh")
        .args([
            "release",
            "create",
            ASSET_RELEASE,
            "--repo",
            WORKFLOW_REPOSITORY,
            "--target",
            WORKFLOW_REF,
            "--title",
            "Pukbot comment assets",
            "--notes",
            "Public media attached to Pukbot comments.",
            "--prerelease",
        ])
        .status()
        .context("failed to launch gh for the media release")?;
    if !status.success() && !asset_release_exists()? {
        bail!("failed to create the Pukbot media release");
    }
    Ok(())
}

fn asset_release_exists() -> Result<bool> {
    let status = Command::new("gh")
        .args([
            "release",
            "view",
            ASSET_RELEASE,
            "--repo",
            WORKFLOW_REPOSITORY,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to inspect the Pukbot media release")?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{MediaInput, resolve};

    #[test]
    fn renders_named_local_image() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("result.png");
        fs::write(&path, b"\x89PNG\r\n\x1a\n").expect("test image should be written");
        let markdown = resolve(
            &MediaInput {
                name: "IMG1".to_owned(),
                path: Some(path),
                url: None,
                alt: Some("result".to_owned()),
            },
            true,
        )
        .expect("image should resolve");
        assert!(markdown.starts_with("![result](https://github.com/pulkitxm/pukbot/"));
    }

    #[test]
    fn renders_video_as_link() {
        let markdown = resolve(
            &MediaInput {
                name: "VIDEO1".to_owned(),
                path: None,
                url: Some("https://example.com/demo.mp4".to_owned()),
                alt: None,
            },
            true,
        )
        .expect("video should resolve");
        assert_eq!(markdown, "[VIDEO1 (video)](https://example.com/demo.mp4)");
    }
}
