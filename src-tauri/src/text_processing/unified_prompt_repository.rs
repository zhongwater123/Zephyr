use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

const MANIFEST_FILE: &str = "manifest.json";
const PROMPT_FILE: &str = "smart_polish.md";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptDocument {
    pub version: String,
    pub sha256: String,
    pub content: String,
}

#[derive(Debug)]
pub struct PromptRepository {
    source: PromptSource,
    manifest: PromptManifest,
}

#[derive(Debug)]
enum PromptSource {
    Directory(PathBuf),
    Bundled,
}

#[derive(Debug, Error)]
pub enum PromptRepositoryError {
    #[error("failed to read prompt manifest: {0}")]
    ManifestRead(String),
    #[error("failed to parse prompt manifest: {0}")]
    ManifestParse(String),
    #[error("invalid prompt manifest: {0}")]
    InvalidManifest(String),
    #[error("failed to read smart polishing prompt: {0}")]
    PromptRead(String),
    #[error("smart polishing prompt is not valid UTF-8")]
    InvalidUtf8,
    #[error("smart polishing prompt hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("smart polishing prompt contains an unsupported include directive")]
    IncludeDirective,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptManifest {
    schema_version: u32,
    prompt: ManifestEntry,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestEntry {
    file: String,
    version: String,
    sha256: String,
}

impl PromptRepository {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, PromptRepositoryError> {
        let root = root.into();
        let bytes = fs::read(root.join(MANIFEST_FILE))
            .map_err(|error| PromptRepositoryError::ManifestRead(error.to_string()))?;
        let manifest: PromptManifest = serde_json::from_slice(&bytes)
            .map_err(|error| PromptRepositoryError::ManifestParse(error.to_string()))?;
        validate_manifest(&manifest)?;
        Ok(Self {
            source: PromptSource::Directory(root),
            manifest,
        })
    }

    pub fn bundled() -> Result<Self, PromptRepositoryError> {
        let manifest: PromptManifest = serde_json::from_slice(include_bytes!(
            "../../resources/prompts/smart_dictation/v2/manifest.json"
        ))
        .map_err(|error| PromptRepositoryError::ManifestParse(error.to_string()))?;
        validate_manifest(&manifest)?;
        Ok(Self {
            source: PromptSource::Bundled,
            manifest,
        })
    }

    pub fn load(&self) -> Result<PromptDocument, PromptRepositoryError> {
        let bytes = match &self.source {
            PromptSource::Directory(root) => fs::read(root.join(&self.manifest.prompt.file))
                .map_err(|error| PromptRepositoryError::PromptRead(error.to_string()))?,
            PromptSource::Bundled => {
                include_bytes!("../../resources/prompts/smart_dictation/v2/smart_polish.md")
                    .to_vec()
            }
        };
        let content = String::from_utf8(bytes).map_err(|_| PromptRepositoryError::InvalidUtf8)?;
        let content = normalize_newlines(&content);
        if contains_include_directive(&content) {
            return Err(PromptRepositoryError::IncludeDirective);
        }
        let actual = normalized_sha256(&content);
        if !actual.eq_ignore_ascii_case(&self.manifest.prompt.sha256) {
            return Err(PromptRepositoryError::HashMismatch {
                expected: self.manifest.prompt.sha256.clone(),
                actual,
            });
        }
        Ok(PromptDocument {
            version: self.manifest.prompt.version.clone(),
            sha256: actual,
            content,
        })
    }
}

fn validate_manifest(manifest: &PromptManifest) -> Result<(), PromptRepositoryError> {
    if manifest.schema_version != 2 {
        return Err(PromptRepositoryError::InvalidManifest(format!(
            "unsupported schema_version {}",
            manifest.schema_version
        )));
    }
    let entry = &manifest.prompt;
    let path = Path::new(&entry.file);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || entry.file != PROMPT_FILE
    {
        return Err(PromptRepositoryError::InvalidManifest(
            "prompt must map to smart_polish.md in the repository root".to_string(),
        ));
    }
    if entry.version.trim().is_empty() {
        return Err(PromptRepositoryError::InvalidManifest(
            "prompt version is empty".to_string(),
        ));
    }
    if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PromptRepositoryError::InvalidManifest(
            "prompt sha256 is invalid".to_string(),
        ));
    }
    Ok(())
}

fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn normalized_sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(normalize_newlines(value).as_bytes()))
}

fn contains_include_directive(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["{{include", "{% include", "!include", "#include"]
        .iter()
        .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_repository(root: &Path, content: &str) {
        fs::write(root.join(PROMPT_FILE), content).unwrap();
        fs::write(
            root.join(MANIFEST_FILE),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": 2,
                "prompt": {
                    "file": PROMPT_FILE,
                    "version": "smart-polish-test",
                    "sha256": normalized_sha256(content),
                },
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn loads_one_versioned_prompt() {
        let directory = tempfile::tempdir().unwrap();
        write_repository(directory.path(), "unified prompt\n");
        let document = PromptRepository::open(directory.path())
            .unwrap()
            .load()
            .unwrap();

        assert_eq!(document.version, "smart-polish-test");
        assert_eq!(document.content, "unified prompt\n");
    }

    #[test]
    fn missing_and_tampered_prompt_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        write_repository(directory.path(), "unified prompt\n");
        let repository = PromptRepository::open(directory.path()).unwrap();

        fs::remove_file(directory.path().join(PROMPT_FILE)).unwrap();
        assert!(matches!(
            repository.load(),
            Err(PromptRepositoryError::PromptRead(_))
        ));

        write_repository(directory.path(), "unified prompt\n");
        let repository = PromptRepository::open(directory.path()).unwrap();
        fs::write(directory.path().join(PROMPT_FILE), "tampered").unwrap();
        assert!(matches!(
            repository.load(),
            Err(PromptRepositoryError::HashMismatch { .. })
        ));
    }

    #[test]
    fn manifest_rejects_traversal_and_include_directives() {
        let directory = tempfile::tempdir().unwrap();
        write_repository(directory.path(), "{{include other.md}}\n");
        let repository = PromptRepository::open(directory.path()).unwrap();
        assert!(matches!(
            repository.load(),
            Err(PromptRepositoryError::IncludeDirective)
        ));

        let manifest = serde_json::json!({
            "schema_version": 2,
            "prompt": {
                "file": "../smart_polish.md",
                "version": "bad",
                "sha256": "0".repeat(64),
            },
        });
        fs::write(
            directory.path().join(MANIFEST_FILE),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            PromptRepository::open(directory.path()),
            Err(PromptRepositoryError::InvalidManifest(_))
        ));
    }

    #[test]
    fn hashes_are_stable_across_lf_and_crlf() {
        assert_eq!(
            normalized_sha256("first\nsecond\n"),
            normalized_sha256("first\r\nsecond\r\n")
        );
    }

    #[test]
    fn bundled_repository_validates_and_loads() {
        let document = PromptRepository::bundled().unwrap().load().unwrap();
        assert_eq!(document.version, "smart-polish-v2");
        assert!(!document.content.trim().is_empty());
    }
}
