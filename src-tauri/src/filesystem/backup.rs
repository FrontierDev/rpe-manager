//! Manager-owned, verified backup sets for future WoW modifications.
//!
//! This service reads source files but never writes into a WoW installation.

use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const BACKUP_METADATA_FILE_NAME: &str = "metadata.json";
const BACKUP_CONTENT_FILE_NAME: &str = "contents.bin";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRequest {
    pub source_path: PathBuf,
    pub installation_id: String,
    pub account_id: Option<String>,
    pub reason: BackupReason,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupReason {
    BeforeAddonModification,
    BeforeSavedVariablesModification,
    Manual,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupMetadata {
    pub id: String,
    pub created_at_unix_millis: u64,
    pub installation_id: String,
    pub account_id: Option<String>,
    pub reason: BackupReason,
    pub source_file_name: String,
    pub byte_length: u64,
}

#[derive(Debug)]
pub struct BackupStore {
    root: PathBuf,
}

impl BackupStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn create_backup(&self, request: BackupRequest) -> Result<BackupMetadata, BackupError> {
        validate_request(&request)?;
        let source_metadata =
            fs::metadata(&request.source_path).map_err(|source| BackupError::ReadSource {
                path: request.source_path.clone(),
                source,
            })?;
        if !source_metadata.is_file() {
            return Err(BackupError::SourceNotFile(request.source_path));
        }
        let source_bytes =
            fs::read(&request.source_path).map_err(|source| BackupError::ReadSource {
                path: request.source_path.clone(),
                source,
            })?;
        let source_file_name = request
            .source_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                BackupError::InvalidRequest("source path has no file name".to_owned())
            })?;

        fs::create_dir_all(&self.root).map_err(|source| BackupError::CreateDirectory {
            path: self.root.clone(),
            source,
        })?;
        let (id, backup_directory) = self.create_backup_directory()?;
        let content_path = backup_directory.join(BACKUP_CONTENT_FILE_NAME);
        write_new_file(&content_path, &source_bytes).map_err(|source| {
            BackupError::WriteBackup {
                path: content_path.clone(),
                source,
            }
        })?;

        let created_at_unix_millis = unix_timestamp_millis()?;
        let metadata = BackupMetadata {
            id,
            created_at_unix_millis,
            installation_id: request.installation_id,
            account_id: request.account_id,
            reason: request.reason,
            source_file_name,
            byte_length: source_bytes.len() as u64,
        };
        let metadata_path = backup_directory.join(BACKUP_METADATA_FILE_NAME);
        let metadata_bytes = serde_json::to_vec_pretty(&metadata)
            .map_err(|error| BackupError::SerializeMetadata(error.to_string()))?;
        write_new_file(&metadata_path, &metadata_bytes).map_err(|source| {
            BackupError::WriteBackup {
                path: metadata_path.clone(),
                source,
            }
        })?;

        self.verify_backup(&backup_directory, &source_bytes, &metadata)?;
        Ok(metadata)
    }

    pub fn list_backups(&self) -> Result<Vec<BackupMetadata>, BackupError> {
        let root_metadata = match fs::metadata(&self.root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(BackupError::ReadBackupRoot {
                    path: self.root.clone(),
                    source,
                })
            }
        };
        if !root_metadata.is_dir() {
            return Err(BackupError::BackupRootNotDirectory(self.root.clone()));
        }

        let mut backups = Vec::new();
        let entries = fs::read_dir(&self.root).map_err(|source| BackupError::ReadBackupRoot {
            path: self.root.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| BackupError::ReadBackupRoot {
                path: self.root.clone(),
                source,
            })?;
            let path = entry.path();
            if !entry
                .file_type()
                .map_err(|source| BackupError::ReadBackupRoot {
                    path: path.clone(),
                    source,
                })?
                .is_dir()
            {
                continue;
            }
            let metadata_path = path.join(BACKUP_METADATA_FILE_NAME);
            let metadata_bytes =
                fs::read(&metadata_path).map_err(|source| BackupError::ReadMetadata {
                    path: metadata_path.clone(),
                    source,
                })?;
            let metadata = serde_json::from_slice(&metadata_bytes).map_err(|error| {
                BackupError::InvalidMetadata {
                    path: metadata_path,
                    message: error.to_string(),
                }
            })?;
            backups.push(metadata);
        }
        backups.sort_by(|left: &BackupMetadata, right| {
            right
                .created_at_unix_millis
                .cmp(&left.created_at_unix_millis)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(backups)
    }

    fn create_backup_directory(&self) -> Result<(String, PathBuf), BackupError> {
        let timestamp = unix_timestamp_millis()?;
        for suffix in 0..10_000_u32 {
            let id = format!("backup-{timestamp}-{suffix:04}");
            let path = self.root.join(&id);
            match fs::create_dir(&path) {
                Ok(()) => return Ok((id, path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(BackupError::CreateDirectory { path, source }),
            }
        }
        Err(BackupError::UniqueNameExhausted(timestamp))
    }

    fn verify_backup(
        &self,
        backup_directory: &Path,
        source_bytes: &[u8],
        expected_metadata: &BackupMetadata,
    ) -> Result<(), BackupError> {
        let content_path = backup_directory.join(BACKUP_CONTENT_FILE_NAME);
        let copied_bytes = fs::read(&content_path).map_err(|source| BackupError::VerifyBackup {
            path: content_path.clone(),
            source,
        })?;
        if copied_bytes != source_bytes {
            return Err(BackupError::ContentMismatch(content_path));
        }

        let metadata_path = backup_directory.join(BACKUP_METADATA_FILE_NAME);
        let metadata_bytes =
            fs::read(&metadata_path).map_err(|source| BackupError::VerifyBackup {
                path: metadata_path.clone(),
                source,
            })?;
        let copied_metadata: BackupMetadata =
            serde_json::from_slice(&metadata_bytes).map_err(|error| {
                BackupError::InvalidMetadata {
                    path: metadata_path.clone(),
                    message: error.to_string(),
                }
            })?;
        if &copied_metadata != expected_metadata {
            return Err(BackupError::MetadataMismatch(metadata_path));
        }
        Ok(())
    }
}

fn write_new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn validate_request(request: &BackupRequest) -> Result<(), BackupError> {
    if request.installation_id.trim().is_empty() {
        return Err(BackupError::InvalidRequest(
            "installation identifier is empty".to_owned(),
        ));
    }
    if request
        .account_id
        .as_deref()
        .is_some_and(|id| id.trim().is_empty())
    {
        return Err(BackupError::InvalidRequest(
            "account identifier is empty".to_owned(),
        ));
    }
    Ok(())
}

fn unix_timestamp_millis() -> Result<u64, BackupError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| BackupError::Clock(error.to_string()))?;
    u64::try_from(duration.as_millis())
        .map_err(|_| BackupError::Clock("timestamp overflow".to_owned()))
}

#[derive(Debug)]
pub enum BackupError {
    BackupRootNotDirectory(PathBuf),
    Clock(String),
    ContentMismatch(PathBuf),
    CreateDirectory { path: PathBuf, source: io::Error },
    InvalidMetadata { path: PathBuf, message: String },
    InvalidRequest(String),
    MetadataMismatch(PathBuf),
    ReadBackupRoot { path: PathBuf, source: io::Error },
    ReadMetadata { path: PathBuf, source: io::Error },
    ReadSource { path: PathBuf, source: io::Error },
    SerializeMetadata(String),
    SourceNotFile(PathBuf),
    UniqueNameExhausted(u64),
    VerifyBackup { path: PathBuf, source: io::Error },
    WriteBackup { path: PathBuf, source: io::Error },
}

impl fmt::Display for BackupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BackupRootNotDirectory(path) => write!(
                formatter,
                "Backup root {} is not a directory",
                path.display()
            ),
            Self::Clock(message) => {
                write!(formatter, "Could not create a backup timestamp: {message}")
            }
            Self::ContentMismatch(path) => write!(
                formatter,
                "Backup content verification failed for {}",
                path.display()
            ),
            Self::CreateDirectory { path, source } => write!(
                formatter,
                "Could not create backup directory {}: {source}",
                path.display()
            ),
            Self::InvalidMetadata { path, message } => write!(
                formatter,
                "Backup metadata {} is invalid: {message}",
                path.display()
            ),
            Self::InvalidRequest(message) => write!(formatter, "Invalid backup request: {message}"),
            Self::MetadataMismatch(path) => write!(
                formatter,
                "Backup metadata verification failed for {}",
                path.display()
            ),
            Self::ReadBackupRoot { path, source } => write!(
                formatter,
                "Could not read backup root {}: {source}",
                path.display()
            ),
            Self::ReadMetadata { path, source } => write!(
                formatter,
                "Could not read backup metadata {}: {source}",
                path.display()
            ),
            Self::ReadSource { path, source } => write!(
                formatter,
                "Could not read backup source {}: {source}",
                path.display()
            ),
            Self::SerializeMetadata(message) => {
                write!(formatter, "Could not serialize backup metadata: {message}")
            }
            Self::SourceNotFile(path) => {
                write!(formatter, "Backup source {} is not a file", path.display())
            }
            Self::UniqueNameExhausted(timestamp) => write!(
                formatter,
                "Could not create a unique backup name for timestamp {timestamp}"
            ),
            Self::VerifyBackup { path, source } => write!(
                formatter,
                "Could not verify backup {}: {source}",
                path.display()
            ),
            Self::WriteBackup { path, source } => write!(
                formatter,
                "Could not write backup {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for BackupError {}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("rpengine-manager-backup-{name}-{unique}"));
        fs::create_dir_all(&directory).expect("create fixture directory");
        directory
    }

    fn request(source_path: PathBuf) -> BackupRequest {
        BackupRequest {
            source_path,
            installation_id: "wow-retail-main".to_owned(),
            account_id: Some("ACCOUNT_ONE".to_owned()),
            reason: BackupReason::BeforeSavedVariablesModification,
        }
    }

    #[test]
    fn creates_a_verified_backup_with_matching_content_and_metadata() {
        let directory = test_directory("success");
        let source = directory.join("source.lua");
        let source_bytes = b"RPEngineManagerDB = { test = true }\n";
        fs::write(&source, source_bytes).expect("write source");
        let store = BackupStore::new(directory.join("manager-backups"));

        let metadata = store
            .create_backup(request(source.clone()))
            .expect("create backup");

        let copied_bytes = fs::read(store.root.join(&metadata.id).join(BACKUP_CONTENT_FILE_NAME))
            .expect("read backup");
        assert_eq!(copied_bytes, source_bytes);
        assert_eq!(
            fs::read(&source).expect("read source after backup"),
            source_bytes
        );
        assert_eq!(metadata.installation_id, "wow-retail-main");
        assert_eq!(metadata.account_id.as_deref(), Some("ACCOUNT_ONE"));
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }

    #[test]
    fn missing_or_non_file_sources_return_explicit_errors() {
        let directory = test_directory("missing");
        let store = BackupStore::new(directory.join("manager-backups"));

        assert!(matches!(
            store.create_backup(request(directory.join("missing.lua"))),
            Err(BackupError::ReadSource { .. })
        ));
        assert!(matches!(
            store.create_backup(request(directory.clone())),
            Err(BackupError::SourceNotFile(_))
        ));
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }

    #[test]
    fn creates_distinct_backup_sets_for_the_same_source_and_lists_them() {
        let directory = test_directory("multiple");
        let source = directory.join("source.lua");
        fs::write(&source, "same source bytes").expect("write source");
        let store = BackupStore::new(directory.join("manager-backups"));

        let first = store
            .create_backup(request(source.clone()))
            .expect("create first backup");
        let second = store
            .create_backup(request(source))
            .expect("create second backup");
        let listed = store.list_backups().expect("list backups");

        assert_ne!(first.id, second.id);
        assert_eq!(listed.len(), 2);
        assert!(listed.contains(&first));
        assert!(listed.contains(&second));
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }
}
