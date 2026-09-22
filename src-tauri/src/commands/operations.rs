//! Narrow Tauri commands for queueing RPE protocol-v1 dataset operations.

use serde::Deserialize;
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
    packages::download::{PackageDownloadError, PackageDownloadService},
    processes::wow::inspect_wow_processes,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueCataloguePackageRequest {
    pub catalogue_id: String,
    /// `None` means the server's current revision.
    pub revision: Option<u64>,
    pub password: Option<String>,
}

/// Retrieves, authenticates, and verifies a catalogue dataset before the
/// payload crosses into the SavedVariables operation queue. Passwords and the
/// temporary unlock token never appear in this command's return value.
#[tauri::command]
pub fn queue_catalogue_package_install(
    app: tauri::AppHandle,
    request: QueueCataloguePackageRequest,
) -> Result<QueueOperationReport, QueueOperationCommandError> {
    let package = PackageDownloadService::production()?.retrieve_selected(
        &request.catalogue_id,
        request.revision,
        request.password.as_deref(),
    )?;
    queue_verified_catalogue_dataset(&app, package)
}

/// The only bridge from a verified remote payload to the existing protocol
/// queue. It deliberately delegates all file safety and atomic-write work.
fn queue_verified_catalogue_dataset(
    app: &tauri::AppHandle,
    package: crate::packages::download::ValidatedPackagePayload,
) -> Result<QueueOperationReport, QueueOperationCommandError> {
    let request = catalogue_dataset_queue_request(package)?;
    let configuration = load_configuration(app)?;
    let backups = backup_store(app).map_err(QueueOperationCommandError::backup)?;
    queue_install_for_selected_accounts(&configuration, &backups, inspect_wow_processes, request)
        .map_err(Into::into)
}

fn catalogue_dataset_queue_request(
    package: crate::packages::download::ValidatedPackagePayload,
) -> Result<QueueInstallDatasetRequest, QueueOperationCommandError> {
    if package.package_type != "dataset" {
        return Err(QueueOperationCommandError::package(
            "only dataset packages can be queued by the current protocol",
        ));
    }
    if package.native_rpe_id.trim().is_empty() {
        return Err(QueueOperationCommandError::package(
            "catalogue dataset is missing its native RPE dataset ID",
        ));
    }
    let revision = u32::try_from(package.revision).map_err(|_| {
        QueueOperationCommandError::package("catalogue revision exceeds protocol-v1 range")
    })?;
    Ok(QueueInstallDatasetRequest {
        request_id: format!("catalogue-{}", crate::elevation::operation_id()),
        catalogue_id: package.catalogue_id,
        dataset_id: package.native_rpe_id,
        revision,
        hash: package.sha256,
        payload: package.payload,
    })
}

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

    fn package(message: impl Into<String>) -> Self {
        Self {
            code: QueueOperationCommandErrorCode::Package,
            message: message.into(),
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
impl From<PackageDownloadError> for QueueOperationCommandError {
    fn from(error: PackageDownloadError) -> Self {
        Self {
            code: QueueOperationCommandErrorCode::Package,
            message: format!("{}: {}", error.code(), error),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueOperationCommandErrorCode {
    BackupLocation,
    Configuration,
    Package,
    Queue,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packages::download::ValidatedPackagePayload;
    use crate::protocol::{OperationEnvelope, OperationKind};
    use sha2::{Digest, Sha256};

    fn package(revision: u64) -> ValidatedPackagePayload {
        ValidatedPackagePayload {
            catalogue_id: "catalogue.example".into(),
            package_type: "dataset".into(),
            native_rpe_id: "dataset.example".into(),
            revision,
            sha256: "a".repeat(64),
            payload: "RPE_DATASET_V1\n{ dataset = {} }".into(),
        }
    }

    #[test]
    fn verified_catalogue_identity_and_payload_become_a_dataset_queue_request() {
        let verified = package(12);
        let request = catalogue_dataset_queue_request(verified.clone()).unwrap();
        assert!(request.request_id.starts_with("catalogue-"));
        assert_eq!(request.catalogue_id, verified.catalogue_id);
        assert_eq!(request.dataset_id, verified.native_rpe_id);
        assert_eq!(request.revision, 12);
        assert_eq!(request.hash, verified.sha256);
        assert_eq!(request.payload, verified.payload);
    }

    #[test]
    fn later_revision_uses_the_same_install_dataset_request_shape() {
        let earlier = catalogue_dataset_queue_request(package(12)).unwrap();
        let later = catalogue_dataset_queue_request(package(13)).unwrap();
        assert_eq!(earlier.catalogue_id, later.catalogue_id);
        assert_eq!(earlier.dataset_id, later.dataset_id);
        assert_eq!(later.revision, earlier.revision + 1);
    }

    #[test]
    fn remote_rulesets_and_missing_native_dataset_ids_are_rejected_before_queueing() {
        let mut ruleset = package(1);
        ruleset.package_type = "ruleset".into();
        assert!(catalogue_dataset_queue_request(ruleset).is_err());
        let mut missing_native_id = package(1);
        missing_native_id.native_rpe_id.clear();
        assert!(catalogue_dataset_queue_request(missing_native_id).is_err());
    }

    #[test]
    fn crlf_verified_payload_is_queued_byte_for_byte_with_its_original_hash() {
        let payload = "RPE_DATASET_V1\r\n{ dataset = { name = \"CRLF\" } }";
        let mut verified = package(4);
        verified.payload = payload.into();
        verified.sha256 = format!("{:x}", Sha256::digest(payload.as_bytes()));
        let request = catalogue_dataset_queue_request(verified).unwrap();
        assert_eq!(request.payload.as_bytes(), payload.as_bytes());
        assert_eq!(
            request.hash,
            format!("{:x}", Sha256::digest(request.payload.as_bytes()))
        );
        assert!(OperationEnvelope {
            request_id: request.request_id,
            operation: OperationKind::InstallDataset,
            catalogue_id: request.catalogue_id,
            dataset_id: request.dataset_id,
            revision: request.revision,
            hash: request.hash,
            payload: Some(request.payload),
        }
        .validate_for_queue()
        .is_ok());
    }
}
