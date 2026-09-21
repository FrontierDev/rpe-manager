//! Commands exposing Manager-owned backup metadata without source-file access.

use serde::Serialize;
use tauri::Manager;

use crate::filesystem::backup::{BackupError, BackupMetadata, BackupStore};

const BACKUP_DIRECTORY_NAME: &str = "backups";

pub(crate) fn backup_store(app: &tauri::AppHandle) -> Result<BackupStore, BackupCommandError> {
    let app_data_directory = app
        .path()
        .app_data_dir()
        .map_err(|error| BackupCommandError::application_data_path(error.to_string()))?;
    Ok(BackupStore::new(
        app_data_directory.join(BACKUP_DIRECTORY_NAME),
    ))
}

#[tauri::command]
pub fn list_manager_backups(
    app: tauri::AppHandle,
) -> Result<Vec<BackupMetadata>, BackupCommandError> {
    backup_store(&app)?.list_backups().map_err(Into::into)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupCommandError {
    pub code: BackupCommandErrorCode,
    pub message: String,
}

impl BackupCommandError {
    fn application_data_path(message: String) -> Self {
        Self {
            code: BackupCommandErrorCode::ApplicationDataPath,
            message,
        }
    }
}

impl From<BackupError> for BackupCommandError {
    fn from(error: BackupError) -> Self {
        Self {
            code: BackupCommandErrorCode::Backup,
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupCommandErrorCode {
    ApplicationDataPath,
    Backup,
}
