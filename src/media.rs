use std::cell::OnceCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const WORKFLOW_REPOSITORY: &str = "pulkitxm/pukbot";
const ASSET_RELEASE: &str = "comment-assets";
const WORKFLOW_REF: &str = "main";
const ATTACHMENT_ENDPOINT: &str = "https://uploads.github.com/user-attachments/assets";
const ATTACHMENT_URL_PREFIX: &str = "https://github.com/user-attachments/assets/";
const PENDING_ATTACHMENT_URL: &str = "https://github.com/user-attachments/assets/pending-upload";
const MAX_MEDIA_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ATTACHMENT_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_ATTACHMENT_VIDEO_BYTES: u64 = 100 * 1024 * 1024;
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
const ATTACHMENT_IMAGE_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
];
const ATTACHMENT_VIDEO_TYPES: &[(&str, &str)] = &[
    ("mp4", "video/mp4"),
    ("mov", "video/quicktime"),
    ("webm", "video/webm"),
];
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

pub struct UploadTarget {
    slug: String,
    repository_id: OnceCell<u64>,
}

impl UploadTarget {
    pub fn new(slug: String) -> Self {
        Self {
            slug,
            repository_id: OnceCell::new(),
        }
    }

    fn repository_id(&self) -> Result<u64> {
        if let Some(id) = self.repository_id.get() {
            return Ok(*id);
        }
        let output = Command::new("gh")
            .args(["api", &format!("repos/{}", self.slug), "--jq", ".id"])
            .output()
            .context("failed to launch gh; install and authenticate GitHub CLI")?;
        if !output.status.success() {
            bail!("failed to resolve {} for the media upload", self.slug);
        }
        let id = String::from_utf8(output.stdout)
            .context("GitHub CLI returned a non-UTF-8 repository id")?
            .trim()
            .parse()
            .context("GitHub CLI returned an unexpected repository id")?;
        let _ = self.repository_id.set(id);
        Ok(id)
    }
}

pub fn supported_extensions() -> Vec<&'static str> {
    SUPPORTED_EXTENSIONS.to_vec()
}

pub fn resolve(input: &MediaInput, target: &UploadTarget, dry_run: bool) -> Result<String> {
    validate_name(&input.name)?;
    match (&input.path, &input.url) {
        (Some(path), None) => resolve_local(input, path, target, dry_run),
        (None, Some(url)) => {
            let (url, extension) = prepare_url(url)?;
            Ok(markdown(label(input), &url, &extension))
        }
        _ => bail!(
            "media {} must provide exactly one of path or url",
            input.name
        ),
    }
}

fn label(input: &MediaInput) -> &str {
    input.alt.as_deref().unwrap_or(&input.name)
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

fn resolve_local(
    input: &MediaInput,
    path: &Path,
    target: &UploadTarget,
    dry_run: bool,
) -> Result<String> {
    let (extension, size) = inspect_local(path)?;
    if let Some(content_type) = content_type(ATTACHMENT_VIDEO_TYPES, &extension) {
        return attach_video(input, path, content_type, size, target, dry_run);
    }
    if let Some(content_type) = content_type(ATTACHMENT_IMAGE_TYPES, &extension) {
        return attach_image(input, path, content_type, size, target, dry_run);
    }
    let url = upload_release_asset(path, &extension, size, dry_run)?;
    Ok(markdown(label(input), &url, &extension))
}

fn inspect_local(path: &Path) -> Result<(String, u64)> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read local media {}", path.display()))?;
    if !metadata.is_file() {
        bail!("local media is not a regular file: {}", path.display());
    }
    if metadata.len() == 0 {
        bail!("local media is empty: {}", path.display());
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
    Ok((extension, metadata.len()))
}

fn content_type(table: &[(&str, &'static str)], extension: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(candidate, _)| *candidate == extension)
        .map(|(_, content_type)| *content_type)
}

fn attach_image(
    input: &MediaInput,
    path: &Path,
    content_type: &str,
    size: u64,
    target: &UploadTarget,
    dry_run: bool,
) -> Result<String> {
    if size > MAX_ATTACHMENT_IMAGE_BYTES {
        bail!("image media must be at most {MAX_ATTACHMENT_IMAGE_BYTES} bytes");
    }
    let url = if dry_run {
        PENDING_ATTACHMENT_URL.to_owned()
    } else {
        upload_attachment(path, content_type, size, target)?
    };
    Ok(format!("![{}]({url})", escape_label(label(input))))
}

fn attach_video(
    input: &MediaInput,
    path: &Path,
    content_type: &str,
    size: u64,
    target: &UploadTarget,
    dry_run: bool,
) -> Result<String> {
    if input.alt.is_some() {
        bail!(
            "media {} renders as a video player, which has no alt text",
            input.name
        );
    }
    if size > MAX_ATTACHMENT_VIDEO_BYTES {
        bail!("video media must be at most {MAX_ATTACHMENT_VIDEO_BYTES} bytes");
    }
    if dry_run {
        return Ok(PENDING_ATTACHMENT_URL.to_owned());
    }
    upload_attachment(path, content_type, size, target)
}

fn upload_attachment(
    path: &Path,
    content_type: &str,
    size: u64,
    target: &UploadTarget,
) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("local media must have a file name")?;
    let endpoint = format!(
        "{ATTACHMENT_ENDPOINT}?name={}&content_type={}&repository_id={}",
        encode(name),
        encode(content_type),
        target.repository_id()?
    );
    let content_length = format!("Content-Length: {size}");
    let output = Command::new("gh")
        .args(["api", &endpoint, "--method", "POST", "--input"])
        .arg(path)
        .args([
            "--header",
            "Content-Type: application/octet-stream",
            "--header",
            &content_length,
            "--header",
            "Accept: application/vnd.github+json",
            "--jq",
            ".url",
        ])
        .output()
        .context("failed to launch gh for the media upload")?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!(
            "failed to attach {} to {}, which requires write access there: {message}",
            path.display(),
            target.slug
        );
    }
    let url = String::from_utf8(output.stdout)
        .context("GitHub CLI returned a non-UTF-8 attachment URL")?
        .trim()
        .to_owned();
    if !url.starts_with(ATTACHMENT_URL_PREFIX) {
        bail!("GitHub returned an unexpected attachment URL");
    }
    Ok(url)
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, UNRESERVED).to_string()
}

fn upload_release_asset(path: &Path, extension: &str, size: u64, dry_run: bool) -> Result<String> {
    if size > MAX_MEDIA_BYTES {
        bail!("local media must be at most {MAX_MEDIA_BYTES} bytes");
    }
    let digest = Sha256::digest(
        fs::read(path).with_context(|| format!("failed to read local media {}", path.display()))?,
    );
    let asset_name = format!("pukbot-{}.{extension}", hex::encode(digest));
    let url = format!(
        "https://github.com/{WORKFLOW_REPOSITORY}/releases/download/{ASSET_RELEASE}/{asset_name}"
    );
    if dry_run {
        return Ok(url);
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
    Ok(url)
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
    let safe_label = escape_label(label);
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

fn escape_label(label: &str) -> String {
    label
        .replace('\\', r"\\")
        .replace('[', r"\[")
        .replace(']', r"\]")
        .replace(['\n', '\r'], " ")
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

    use super::{MediaInput, UploadTarget, resolve};

    fn target() -> UploadTarget {
        UploadTarget::new("owner/repository".to_owned())
    }

    #[test]
    fn renders_named_local_image_as_an_attachment() {
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
            &target(),
            true,
        )
        .expect("image should resolve");
        assert_eq!(
            markdown,
            "![result](https://github.com/user-attachments/assets/pending-upload)"
        );
    }

    #[test]
    fn renders_local_video_as_a_bare_player_url() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("demo.mp4");
        fs::write(&path, b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom")
            .expect("test video should be written");
        let markdown = resolve(
            &MediaInput {
                name: "VIDEO1".to_owned(),
                path: Some(path),
                url: None,
                alt: None,
            },
            &target(),
            true,
        )
        .expect("video should resolve");
        assert_eq!(
            markdown,
            "https://github.com/user-attachments/assets/pending-upload"
        );
    }

    #[test]
    fn rejects_alt_text_on_a_video() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("demo.mp4");
        fs::write(&path, b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom")
            .expect("test video should be written");
        assert!(
            resolve(
                &MediaInput {
                    name: "VIDEO1".to_owned(),
                    path: Some(path),
                    url: None,
                    alt: Some("demo".to_owned()),
                },
                &target(),
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn keeps_release_uploads_for_unattachable_types() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("logs.txt");
        fs::write(&path, b"finished\n").expect("test document should be written");
        let markdown = resolve(
            &MediaInput {
                name: "LOGS".to_owned(),
                path: Some(path),
                url: None,
                alt: Some("logs".to_owned()),
            },
            &target(),
            true,
        )
        .expect("document should resolve");
        assert!(
            markdown.starts_with("[logs](https://github.com/pulkitxm/pukbot/releases/download/")
        );
    }

    #[test]
    fn renders_remote_video_as_link() {
        let markdown = resolve(
            &MediaInput {
                name: "VIDEO1".to_owned(),
                path: None,
                url: Some("https://example.com/demo.mp4".to_owned()),
                alt: None,
            },
            &target(),
            true,
        )
        .expect("video should resolve");
        assert_eq!(markdown, "[VIDEO1 (video)](https://example.com/demo.mp4)");
    }
}
