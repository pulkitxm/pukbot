use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use base64::Engine as _;

use crate::model::MAX_RELEASE_ASSET_BYTES;

pub struct ReleaseAsset {
    pub name: String,
    pub label: Option<String>,
    pub content_type: String,
    pub content_base64: String,
}

pub fn prepare(
    path: &Path,
    name: Option<String>,
    label: Option<String>,
    content_type: Option<String>,
) -> Result<ReleaseAsset> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to read release asset {}", path.display()))?;
    if !metadata.is_file() {
        bail!("release asset is not a regular file: {}", path.display());
    }
    if metadata.len() == 0 || metadata.len() > MAX_RELEASE_ASSET_BYTES as u64 {
        bail!("release asset must be between 1 and {MAX_RELEASE_ASSET_BYTES} bytes");
    }
    let content = fs::read(path)
        .with_context(|| format!("failed to read release asset {}", path.display()))?;
    let name = name.unwrap_or_else(|| {
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned()
    });
    let content_type = content_type.unwrap_or_else(|| infer_content_type(path, &content));
    let content_base64 = base64::engine::general_purpose::STANDARD.encode(content);
    Ok(ReleaseAsset {
        name,
        label,
        content_type,
        content_base64,
    })
}

fn infer_content_type(path: &Path, content: &[u8]) -> String {
    if let Some(kind) = infer::get(content) {
        return kind.mime_type().to_owned();
    }
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => "application/json".to_owned(),
        Some("md") => "text/markdown".to_owned(),
        Some("txt") => "text/plain".to_owned(),
        Some("csv") => "text/csv".to_owned(),
        _ => "application/octet-stream".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::Engine as _;

    use super::prepare;

    #[test]
    fn prepares_release_asset() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("checksums.txt");
        fs::write(&path, b"abc123\n").expect("fixture should be written");
        let asset =
            prepare(&path, None, Some("checksums".to_owned()), None).expect("asset should prepare");
        assert_eq!(asset.name, "checksums.txt");
        assert_eq!(asset.content_type, "text/plain");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(asset.content_base64)
                .expect("asset should decode"),
            b"abc123\n"
        );
    }

    #[test]
    fn rejects_empty_release_asset() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("empty.bin");
        fs::write(&path, []).expect("fixture should be written");
        assert!(prepare(&path, None, None, None).is_err());
    }
}
