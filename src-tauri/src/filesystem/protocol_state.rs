//! Read-only protocol-v1 state and request reconciliation from SavedVariables.
//!
//! This module reads only `RPEngineManagerDB`. It never inspects
//! `RPEngineDatasetDB`, and on-disk state is marked non-authoritative while WoW
//! is running because SavedVariables may not have been persisted yet.

use std::{fmt, fs, io, path::Path};

use serde::Serialize;

use crate::{
    configuration::ManagerConfiguration,
    filesystem::{
        account_targets::{
            resolve_selected_saved_variables_targets, SavedVariablesAccountTarget,
            SavedVariablesTargetError,
        },
        saved_variables::{parse_manager_state, ManagerSavedVariables, SavedVariablesError},
    },
    processes::wow::WowModificationSafetyState,
    protocol::{
        validate_request_id, InstalledPackage, OperationEnvelope, OperationResult,
        OperationResultStatus, PackageType, ProtocolState, ProtocolValidationError,
    },
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedProtocolState {
    /// True when a WoW client is running and the disk snapshot may be behind
    /// the addon's in-memory state.
    pub is_stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_reason: Option<String>,
    pub accounts: Vec<AccountProtocolState>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProtocolState {
    pub account_id: String,
    pub availability: ProtocolAvailability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<u32>,
    pub pending_operations: Vec<PendingOperation>,
    pub operation_results: Vec<OperationResult>,
    pub installed_packages: Vec<InstalledPackageRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolStateError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolAvailability {
    Absent,
    Available,
    Malformed,
    SourceUnavailable,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingOperation {
    pub request_id: String,
    pub operation: crate::protocol::OperationKind,
    pub catalogue_id: String,
    pub dataset_id: String,
    pub revision: u32,
    pub hash: String,
    pub has_payload: bool,
}

impl From<&OperationEnvelope> for PendingOperation {
    fn from(operation: &OperationEnvelope) -> Self {
        Self {
            request_id: operation.request_id.clone(),
            operation: operation.operation,
            catalogue_id: operation.catalogue_id.clone(),
            dataset_id: operation.dataset_id.clone(),
            revision: operation.revision,
            hash: operation.hash.clone(),
            has_payload: operation.payload.is_some(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackageRecord {
    pub catalogue_id: String,
    pub package_type: PackageType,
    pub dataset_id: String,
    pub revision: u32,
    pub hash: String,
    pub installed_at: u64,
}

impl InstalledPackageRecord {
    fn from_entry(catalogue_id: &str, package: &InstalledPackage) -> Self {
        Self {
            catalogue_id: catalogue_id.to_owned(),
            package_type: package.package_type,
            dataset_id: package.dataset_id.clone(),
            revision: package.revision,
            hash: package.hash.clone(),
            installed_at: package.installed_at,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolStateError {
    pub code: ProtocolStateErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolStateErrorCode {
    Malformed,
    SourceRead,
    SourceNotFile,
    Unsupported,
}

/// Reads one disk snapshot per selected account. A malformed or missing file
/// on one account is returned for that account and does not hide other account
/// state.
pub fn read_selected_protocol_state(
    configuration: &ManagerConfiguration,
    safety: &WowModificationSafetyState,
) -> Result<SelectedProtocolState, ProtocolStateReadError> {
    let targets = resolve_selected_saved_variables_targets(configuration)
        .map_err(ProtocolStateReadError::TargetResolution)?;
    let is_stale = safety.is_wow_running;
    let stale_reason = is_stale.then(|| {
        if safety.matching_process_names.is_empty() {
            "World of Warcraft is running; this is an on-disk snapshot and may be stale.".to_owned()
        } else {
            format!(
                "World of Warcraft is running ({}); this on-disk snapshot may be stale.",
                safety.matching_process_names.join(", ")
            )
        }
    });

    Ok(SelectedProtocolState {
        is_stale,
        stale_reason,
        accounts: targets.iter().map(read_account_protocol_state).collect(),
    })
}

fn read_account_protocol_state(target: &SavedVariablesAccountTarget) -> AccountProtocolState {
    match read_source(&target.saved_variables_path) {
        Ok(source) => match parse_manager_state(&source) {
            Ok(ManagerSavedVariables::Absent) => AccountProtocolState::absent(&target.account_id),
            Ok(ManagerSavedVariables::Present(state)) => {
                AccountProtocolState::available(&target.account_id, state)
            }
            Err(error) => AccountProtocolState::parse_error(&target.account_id, error),
        },
        Err(error) => AccountProtocolState::source_error(&target.account_id, error),
    }
}

fn read_source(path: &Path) -> Result<String, SourceReadError> {
    let metadata = fs::metadata(path).map_err(SourceReadError::Read)?;
    if !metadata.is_file() {
        return Err(SourceReadError::NotFile);
    }
    fs::read_to_string(path).map_err(SourceReadError::Read)
}

impl AccountProtocolState {
    fn absent(account_id: &str) -> Self {
        Self {
            account_id: account_id.to_owned(),
            availability: ProtocolAvailability::Absent,
            protocol_version: None,
            pending_operations: Vec::new(),
            operation_results: Vec::new(),
            installed_packages: Vec::new(),
            error: None,
        }
    }

    fn available(account_id: &str, state: ProtocolState) -> Self {
        Self {
            account_id: account_id.to_owned(),
            availability: ProtocolAvailability::Available,
            protocol_version: Some(state.protocol_version),
            pending_operations: state
                .pending_operations
                .iter()
                .map(PendingOperation::from)
                .collect(),
            operation_results: state.operation_results.into_values().collect(),
            installed_packages: state
                .installed_packages
                .iter()
                .map(|(catalogue_id, package)| {
                    InstalledPackageRecord::from_entry(catalogue_id, package)
                })
                .collect(),
            error: None,
        }
    }

    fn parse_error(account_id: &str, error: SavedVariablesError) -> Self {
        let (availability, code) = match error {
            SavedVariablesError::UnsupportedProtocolVersion(_) => (
                ProtocolAvailability::Unsupported,
                ProtocolStateErrorCode::Unsupported,
            ),
            _ => (
                ProtocolAvailability::Malformed,
                ProtocolStateErrorCode::Malformed,
            ),
        };
        Self {
            account_id: account_id.to_owned(),
            availability,
            protocol_version: None,
            pending_operations: Vec::new(),
            operation_results: Vec::new(),
            installed_packages: Vec::new(),
            error: Some(ProtocolStateError {
                code,
                message: error.to_string(),
            }),
        }
    }

    fn source_error(account_id: &str, error: SourceReadError) -> Self {
        let (code, message) = match error {
            SourceReadError::NotFile => (
                ProtocolStateErrorCode::SourceNotFile,
                "SavedVariables source is not a file".to_owned(),
            ),
            SourceReadError::Read(error) => (
                ProtocolStateErrorCode::SourceRead,
                format!("Could not read SavedVariables source: {error}"),
            ),
        };
        Self {
            account_id: account_id.to_owned(),
            availability: ProtocolAvailability::SourceUnavailable,
            protocol_version: None,
            pending_operations: Vec::new(),
            operation_results: Vec::new(),
            installed_packages: Vec::new(),
            error: Some(ProtocolStateError { code, message }),
        }
    }
}

enum SourceReadError {
    NotFile,
    Read(io::Error),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestReconciliationReport {
    pub request_id: String,
    pub is_stale: bool,
    pub accounts: Vec<AccountRequestReconciliation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRequestReconciliation {
    pub account_id: String,
    pub status: RequestReconciliationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<OperationResult>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestReconciliationStatus {
    /// Returned by the queue command before RPE has persisted any later state.
    Queued,
    PendingInRpe,
    Succeeded,
    Failed,
    StaleDiskState,
    UnknownNotYetPersisted,
}

pub fn reconcile_request(
    snapshot: &SelectedProtocolState,
    request_id: &str,
) -> Result<RequestReconciliationReport, ProtocolStateReadError> {
    validate_request_id(request_id).map_err(ProtocolStateReadError::InvalidRequestId)?;
    Ok(RequestReconciliationReport {
        request_id: request_id.to_owned(),
        is_stale: snapshot.is_stale,
        accounts: snapshot
            .accounts
            .iter()
            .map(|account| reconcile_account(snapshot.is_stale, account, request_id))
            .collect(),
    })
}

fn reconcile_account(
    is_stale: bool,
    account: &AccountProtocolState,
    request_id: &str,
) -> AccountRequestReconciliation {
    if is_stale {
        return AccountRequestReconciliation {
            account_id: account.account_id.clone(),
            status: RequestReconciliationStatus::StaleDiskState,
            result: None,
        };
    }
    if account.availability != ProtocolAvailability::Available {
        return AccountRequestReconciliation {
            account_id: account.account_id.clone(),
            status: RequestReconciliationStatus::UnknownNotYetPersisted,
            result: None,
        };
    }
    if let Some(result) = account
        .operation_results
        .iter()
        .find(|result| result.request_id == request_id)
    {
        return AccountRequestReconciliation {
            account_id: account.account_id.clone(),
            status: match result.status {
                OperationResultStatus::Succeeded => RequestReconciliationStatus::Succeeded,
                OperationResultStatus::Failed => RequestReconciliationStatus::Failed,
            },
            result: Some(result.clone()),
        };
    }
    let status = if account
        .pending_operations
        .iter()
        .any(|operation| operation.request_id == request_id)
    {
        RequestReconciliationStatus::PendingInRpe
    } else {
        RequestReconciliationStatus::UnknownNotYetPersisted
    };
    AccountRequestReconciliation {
        account_id: account.account_id.clone(),
        status,
        result: None,
    }
}

#[derive(Debug)]
pub enum ProtocolStateReadError {
    InvalidRequestId(ProtocolValidationError),
    TargetResolution(SavedVariablesTargetError),
}

impl fmt::Display for ProtocolStateReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequestId(error) => write!(formatter, "Invalid request ID: {error}"),
            Self::TargetResolution(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ProtocolStateReadError {}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::{
        configuration::{InstallationAvailability, WowInstallation, WowProduct},
        filesystem::saved_variables::serialize_manager_assignment,
        processes::wow::safety_state_from_process_names,
        protocol::{
            OperationKind, OperationResultStatus, ProtocolErrorCode, ProtocolFailure,
            ResultOperation,
        },
    };

    const HASH: &str = "2aee76c57aa954b6ca769bb1c1ff839c0805cea4aa4ffb3ebe153fe48e3c71de";

    struct Fixture {
        directory: PathBuf,
        configuration: ManagerConfiguration,
    }

    fn fixture(name: &str, accounts: &[(&str, &str)]) -> Fixture {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("rpengine-manager-protocol-{name}-{unique}"));
        let installation_path = directory.join("_retail_");
        fs::create_dir_all(installation_path.join("Data")).unwrap();
        fs::write(installation_path.join("Wow.exe"), "fixture executable").unwrap();
        for (account_id, source) in accounts {
            let path = installation_path
                .join("WTF/Account")
                .join(account_id)
                .join("SavedVariables/RPEngine2.lua");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
        Fixture {
            directory,
            configuration: ManagerConfiguration {
                installations: vec![WowInstallation {
                    id: "retail".to_owned(),
                    product: Some(WowProduct::Retail),
                    path: installation_path,
                    availability: InstallationAvailability::Available,
                }],
                selected_installation_id: Some("retail".to_owned()),
                selected_account_ids: [(
                    "retail".to_owned(),
                    accounts.iter().map(|(id, _)| (*id).to_owned()).collect(),
                )]
                .into_iter()
                .collect(),
                ..ManagerConfiguration::default()
            },
        }
    }

    fn safe_state() -> WowModificationSafetyState {
        safety_state_from_process_names(["explorer.exe"])
    }

    fn pending_operation(request_id: &str) -> OperationEnvelope {
        OperationEnvelope {
            request_id: request_id.to_owned(),
            operation: OperationKind::InstallDataset,
            catalogue_id: "esarus-core".to_owned(),
            dataset_id: "f82db71a".to_owned(),
            revision: 14,
            hash: HASH.to_owned(),
            payload: Some("RPE_DATASET_V1\nfixture payload\n".to_owned()),
        }
    }

    fn result(request_id: &str, status: OperationResultStatus) -> OperationResult {
        OperationResult {
            request_id: request_id.to_owned(),
            operation: ResultOperation::InstallDataset,
            status,
            catalogue_id: Some("esarus-core".to_owned()),
            dataset_id: Some("f82db71a".to_owned()),
            revision: Some(14),
            hash: Some(HASH.to_owned()),
            error: (status == OperationResultStatus::Failed).then(|| ProtocolFailure {
                code: ProtocolErrorCode::ImportRejected,
                detail: "fixture failure".to_owned(),
            }),
        }
    }

    fn source_for(state: ProtocolState) -> String {
        format!(
            "RPEngineProfilesDB = {{ keep = true }}\n{}\n",
            serialize_manager_assignment(&state).unwrap()
        )
    }

    #[test]
    fn reads_absent_valid_and_pending_manager_state() {
        let mut queued = ProtocolState::default();
        queued
            .pending_operations
            .push(pending_operation("pending-1"));
        let fixture = fixture(
            "states",
            &[
                ("ABSENT", "RPEngineProfilesDB = {}\n"),
                ("EMPTY", &source_for(ProtocolState::default())),
                ("PENDING", &source_for(queued)),
            ],
        );

        let state = read_selected_protocol_state(&fixture.configuration, &safe_state()).unwrap();

        assert_eq!(state.accounts[0].availability, ProtocolAvailability::Absent);
        assert_eq!(state.accounts[1].protocol_version, Some(1));
        assert!(state.accounts[1].pending_operations.is_empty());
        assert_eq!(
            state.accounts[2].pending_operations[0].request_id,
            "pending-1"
        );
        let reconciliation = reconcile_request(&state, "pending-1").unwrap();
        assert_eq!(
            reconciliation.accounts[2].status,
            RequestReconciliationStatus::PendingInRpe
        );
        fs::remove_dir_all(fixture.directory).unwrap();
    }

    #[test]
    fn reads_terminal_results_and_installed_package_manifest() {
        let mut state = ProtocolState::default();
        state.operation_results.insert(
            "success-1".to_owned(),
            result("success-1", OperationResultStatus::Succeeded),
        );
        state.operation_results.insert(
            "failure-1".to_owned(),
            result("failure-1", OperationResultStatus::Failed),
        );
        state.installed_packages.insert(
            "esarus-core".to_owned(),
            InstalledPackage {
                package_type: PackageType::Dataset,
                dataset_id: "f82db71a".to_owned(),
                revision: 14,
                hash: HASH.to_owned(),
                installed_at: 1_789_940_000,
            },
        );
        let fixture = fixture("results", &[("ACCOUNT", &source_for(state))]);

        let snapshot = read_selected_protocol_state(&fixture.configuration, &safe_state()).unwrap();
        assert_eq!(snapshot.accounts[0].operation_results.len(), 2);
        assert_eq!(snapshot.accounts[0].installed_packages[0].revision, 14);
        assert_eq!(
            snapshot.accounts[0].installed_packages[0].installed_at,
            1_789_940_000
        );
        assert_eq!(
            reconcile_request(&snapshot, "success-1").unwrap().accounts[0].status,
            RequestReconciliationStatus::Succeeded
        );
        assert_eq!(
            reconcile_request(&snapshot, "failure-1").unwrap().accounts[0].status,
            RequestReconciliationStatus::Failed
        );
        fs::remove_dir_all(fixture.directory).unwrap();
    }

    #[test]
    fn exposes_malformed_and_unsupported_accounts_without_hiding_other_accounts() {
        let fixture = fixture(
            "errors",
            &[
                ("MALFORMED", "RPEngineManagerDB = { protocolVersion = 1\n"),
                (
                    "UNSUPPORTED",
                    "RPEngineManagerDB = { protocolVersion = 2, futureField = true }\n",
                ),
                ("VALID", &source_for(ProtocolState::default())),
            ],
        );

        let snapshot = read_selected_protocol_state(&fixture.configuration, &safe_state()).unwrap();
        assert_eq!(
            snapshot.accounts[0].availability,
            ProtocolAvailability::Malformed
        );
        assert_eq!(
            snapshot.accounts[1].availability,
            ProtocolAvailability::Unsupported
        );
        assert_eq!(
            snapshot.accounts[2].availability,
            ProtocolAvailability::Available
        );
        assert!(snapshot.accounts[0].error.is_some());
        fs::remove_dir_all(fixture.directory).unwrap();
    }

    #[test]
    fn running_wow_marks_disk_results_as_stale_and_not_authoritative() {
        let mut state = ProtocolState::default();
        state.operation_results.insert(
            "success-1".to_owned(),
            result("success-1", OperationResultStatus::Succeeded),
        );
        let fixture = fixture("stale", &[("ACCOUNT", &source_for(state))]);
        let running = safety_state_from_process_names(["Wow.exe"]);

        let snapshot = read_selected_protocol_state(&fixture.configuration, &running).unwrap();
        assert!(snapshot.is_stale);
        assert!(snapshot.stale_reason.as_ref().unwrap().contains("Wow.exe"));
        assert_eq!(
            reconcile_request(&snapshot, "success-1").unwrap().accounts[0].status,
            RequestReconciliationStatus::StaleDiskState
        );
        fs::remove_dir_all(fixture.directory).unwrap();
    }
}
