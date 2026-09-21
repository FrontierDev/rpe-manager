//! Narrow Tauri commands for queueing RPE protocol-v1 dataset operations.

use serde::Serialize;

use crate::{
    commands::{
        backups::{backup_store, BackupCommandError},
        configuration::configuration_store,
    },
    configuration::ConfigurationCommandError,
    filesystem::operation_queue::{
        queue_install_for_selected_accounts, queue_install_ruleset_for_selected_accounts,
        queue_remove_for_selected_accounts, QueueInstallDatasetRequest, QueueInstallRulesetRequest,
        QueueOperationError, QueueOperationReport, QueueRemoveDatasetRequest,
    },
    processes::wow::inspect_wow_processes,
};

#[tauri::command]
pub fn queue_install_dataset(
    app: tauri::AppHandle,
    request: QueueInstallDatasetRequest,
) -> Result<QueueOperationReport, QueueOperationCommandError> {
    let configuration = load_configuration(&app)?;
    let backups = backup_store(&app).map_err(QueueOperationCommandError::backup)?;
    queue_install_for_selected_accounts(&configuration, &backups, inspect_wow_processes, request)
        .map_err(Into::into)
}

#[tauri::command]
pub fn queue_remove_dataset(
    app: tauri::AppHandle,
    request: QueueRemoveDatasetRequest,
) -> Result<QueueOperationReport, QueueOperationCommandError> {
    let configuration = load_configuration(&app)?;
    let backups = backup_store(&app).map_err(QueueOperationCommandError::backup)?;
    queue_remove_for_selected_accounts(&configuration, &backups, inspect_wow_processes, request)
        .map_err(Into::into)
}

#[tauri::command]
pub fn queue_install_ruleset(
    app: tauri::AppHandle,
    request: QueueInstallRulesetRequest,
) -> Result<QueueOperationReport, QueueOperationCommandError> {
    let configuration = load_configuration(&app)?;
    let backups = backup_store(&app).map_err(QueueOperationCommandError::backup)?;
    queue_install_ruleset_for_selected_accounts(
        &configuration,
        &backups,
        inspect_wow_processes,
        request,
    )
    .map_err(Into::into)
}

fn load_configuration(
    app: &tauri::AppHandle,
) -> Result<crate::configuration::ManagerConfiguration, QueueOperationCommandError> {
    configuration_store(app)
        .map_err(QueueOperationCommandError::configuration)?
        .load()
        .map_err(|error| QueueOperationCommandError::configuration(error.into()))
        .map(|load| load.configuration)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperationCommandError {
    pub code: QueueOperationCommandErrorCode,
    pub message: String,
}

impl QueueOperationCommandError {
    fn configuration(error: ConfigurationCommandError) -> Self {
        Self {
            code: QueueOperationCommandErrorCode::Configuration,
            message: error.message,
        }
    }

    fn backup(error: BackupCommandError) -> Self {
        Self {
            code: QueueOperationCommandErrorCode::BackupLocation,
            message: error.message,
        }
    }
}

impl From<QueueOperationError> for QueueOperationCommandError {
    fn from(error: QueueOperationError) -> Self {
        Self {
            code: QueueOperationCommandErrorCode::Queue,
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueOperationCommandErrorCode {
    BackupLocation,
    Configuration,
    Queue,
}
