//! Safe, per-account transactions for queuing RPE external operations.
//!
//! The only SavedVariables mutation this module makes is replacing the
//! `RPEngineManagerDB` assignment through the data-only parser. The addon owns
//! dataset contents, installed-package state, and operation results.

use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    configuration::ManagerConfiguration,
    filesystem::{
        account_targets::{
            resolve_selected_saved_variables_targets, SavedVariablesAccountTarget,
            SavedVariablesTargetError,
        },
        backup::{BackupMetadata, BackupReason, BackupRequest, BackupStore},
        saved_variables::{
            parse_manager_state, replace_manager_assignment, ManagerSavedVariables,
            SavedVariablesError,
        },
    },
    processes::wow::WowModificationSafetyState,
    protocol::{OperationEnvelope, OperationKind, ProtocolState, ProtocolValidationError},
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueInstallDatasetRequest {
    pub request_id: String,
    pub catalogue_id: String,
    pub dataset_id: String,
    pub revision: u32,
    pub hash: String,
    pub payload: String,
}

impl QueueInstallDatasetRequest {
    fn into_operation(self) -> OperationEnvelope {
        OperationEnvelope {
            request_id: self.request_id,
            operation: OperationKind::InstallDataset,
            catalogue_id: self.catalogue_id,
            dataset_id: self.dataset_id,
            revision: self.revision,
            hash: self.hash,
            payload: Some(self.payload),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueRemoveDatasetRequest {
    pub request_id: String,
    pub catalogue_id: String,
    pub dataset_id: String,
    pub revision: u32,
    pub hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueInstallRulesetRequest {
    pub request_id: String,
    pub payload: String,
}

impl QueueInstallRulesetRequest {
    fn into_operation(self) -> OperationEnvelope {
        let hash = format!("{:x}", Sha256::digest(self.payload.as_bytes()));
        OperationEnvelope {
            request_id: self.request_id,
            operation: OperationKind::InstallRuleset,
            // Rulesets are local imports. These stable envelope fields retain
            // the v1 identity shape; RPE derives canonical ruleset identity.
            catalogue_id: "local".to_owned(),
            dataset_id: "ruleset-import".to_owned(),
            revision: 1,
            hash,
            payload: Some(self.payload),
        }
    }
}

impl QueueRemoveDatasetRequest {
    fn into_operation(self) -> OperationEnvelope {
        OperationEnvelope {
            request_id: self.request_id,
            operation: OperationKind::RemoveDataset,
            catalogue_id: self.catalogue_id,
            dataset_id: self.dataset_id,
            revision: self.revision,
            hash: self.hash,
            payload: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedOperationIdentity {
    pub request_id: String,
    pub operation: OperationKind,
    pub catalogue_id: String,
    pub dataset_id: String,
    pub revision: u32,
    pub hash: String,
}

impl From<&OperationEnvelope> for QueuedOperationIdentity {
    fn from(operation: &OperationEnvelope) -> Self {
        Self {
            request_id: operation.request_id.clone(),
            operation: operation.operation,
            catalogue_id: operation.catalogue_id.clone(),
            dataset_id: operation.dataset_id.clone(),
            revision: operation.revision,
            hash: operation.hash.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperationReport {
    pub operation: QueuedOperationIdentity,
    pub accounts: Vec<AccountQueueResult>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountQueueResult {
    pub account_id: String,
    pub status: AccountQueueStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AccountQueueError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountQueueStatus {
    Queued,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountQueueError {
    pub code: AccountQueueErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountQueueErrorCode {
    WowRunning,
    SourceRead,
    SourceNotFile,
    Parse,
    DuplicateRequestId,
    Backup,
    TemporaryWrite,
    TemporaryValidation,
    AtomicReplacement,
}

/// Queues an install operation for every selected account. `safety_check` is
/// called immediately before each account transaction so a changed process
/// state cannot be reused from an earlier account.
pub fn queue_install_for_selected_accounts<F>(
    configuration: &ManagerConfiguration,
    backups: &BackupStore,
    safety_check: F,
    request: QueueInstallDatasetRequest,
) -> Result<QueueOperationReport, QueueOperationError>
where
    F: FnMut() -> WowModificationSafetyState,
{
    queue_operation_for_selected_accounts(
        configuration,
        backups,
        safety_check,
        request.into_operation(),
    )
}

/// Queues a removal operation for every selected account.
pub fn queue_remove_for_selected_accounts<F>(
    configuration: &ManagerConfiguration,
    backups: &BackupStore,
    safety_check: F,
    request: QueueRemoveDatasetRequest,
) -> Result<QueueOperationReport, QueueOperationError>
where
    F: FnMut() -> WowModificationSafetyState,
{
    queue_operation_for_selected_accounts(
        configuration,
        backups,
        safety_check,
        request.into_operation(),
    )
}

pub fn queue_install_ruleset_for_selected_accounts<F>(
    configuration: &ManagerConfiguration,
    backups: &BackupStore,
    safety_check: F,
    request: QueueInstallRulesetRequest,
) -> Result<QueueOperationReport, QueueOperationError>
where
    F: FnMut() -> WowModificationSafetyState,
{
    queue_operation_for_selected_accounts(
        configuration,
        backups,
        safety_check,
        request.into_operation(),
    )
}

fn queue_operation_for_selected_accounts<F>(
    configuration: &ManagerConfiguration,
    backups: &BackupStore,
    mut safety_check: F,
    operation: OperationEnvelope,
) -> Result<QueueOperationReport, QueueOperationError>
where
    F: FnMut() -> WowModificationSafetyState,
{
    operation
        .validate_for_queue()
        .map_err(QueueOperationError::InvalidOperation)?;
    let targets = resolve_selected_saved_variables_targets(configuration)
        .map_err(QueueOperationError::TargetResolution)?;
    let identity = QueuedOperationIdentity::from(&operation);
    let mut accounts = Vec::with_capacity(targets.len());

    for target in targets {
        // A newly discovered account may not have loaded RPE yet, so it has no
        // SavedVariables file to host the protocol root. Leave it untouched.
        if matches!(target.saved_variables_path.try_exists(), Ok(false)) {
            continue;
        }
        let safety = safety_check();
        if !safety.can_modify_wow_files {
            accounts.push(AccountQueueResult::failure(
                target.account_id,
                AccountQueueError::wow_running(&safety),
            ));
            continue;
        }

        match queue_one_account(&target, &operation, backups, validate_temporary_file) {
            Ok(backup) => accounts.push(AccountQueueResult::queued(target.account_id, backup)),
            Err(error) => accounts.push(AccountQueueResult::failure(target.account_id, error)),
        }
    }

    Ok(QueueOperationReport {
        operation: identity,
        accounts,
    })
}

impl AccountQueueResult {
    fn queued(account_id: String, backup: BackupMetadata) -> Self {
        Self {
            account_id,
            status: AccountQueueStatus::Queued,
            backup_id: Some(backup.id),
            error: None,
        }
    }

    fn failure(account_id: String, error: AccountQueueError) -> Self {
        Self {
            account_id,
            status: AccountQueueStatus::Failed,
            backup_id: None,
            error: Some(error),
        }
    }
}

fn queue_one_account<F>(
    target: &SavedVariablesAccountTarget,
    operation: &OperationEnvelope,
    backups: &BackupStore,
    validate_staged: F,
) -> Result<BackupMetadata, AccountQueueError>
where
    F: Fn(&Path, &ProtocolState) -> Result<(), AccountQueueError>,
{
    let source = read_source(&target.saved_variables_path)?;
    let state = match parse_manager_state(&source).map_err(AccountQueueError::parse)? {
        ManagerSavedVariables::Absent => ProtocolState::default(),
        ManagerSavedVariables::Present(state) => state,
    };
    reject_duplicate_request_id(&state, &operation.request_id)?;

    let backup = backups
        .create_backup(BackupRequest {
            source_path: target.saved_variables_path.clone(),
            installation_id: target.installation_id.clone(),
            account_id: Some(target.account_id.clone()),
            reason: BackupReason::BeforeSavedVariablesModification,
        })
        .map_err(AccountQueueError::backup)?;

    let mut updated_state = state;
    updated_state.pending_operations.push(operation.clone());
    let replacement =
        replace_manager_assignment(&source, &updated_state).map_err(AccountQueueError::parse)?;
    let temporary_path =
        write_temporary_source(&target.saved_variables_path, replacement.as_bytes())?;

    if let Err(error) = validate_staged(&temporary_path, &updated_state) {
        remove_temporary_file(&temporary_path);
        return Err(error);
    }

    if let Err(error) = replace_file_atomically(&temporary_path, &target.saved_variables_path) {
        remove_temporary_file(&temporary_path);
        return Err(AccountQueueError::atomic_replacement(error));
    }
    Ok(backup)
}

fn read_source(path: &Path) -> Result<String, AccountQueueError> {
    let metadata =
        fs::metadata(path).map_err(|error| AccountQueueError::source_read(path, error))?;
    if !metadata.is_file() {
        return Err(AccountQueueError::source_not_file(path));
    }
    fs::read_to_string(path).map_err(|error| AccountQueueError::source_read(path, error))
}

fn reject_duplicate_request_id(
    state: &ProtocolState,
    request_id: &str,
) -> Result<(), AccountQueueError> {
    if state
        .pending_operations
        .iter()
        .any(|operation| operation.request_id == request_id)
        || state.operation_results.contains_key(request_id)
    {
        Err(AccountQueueError::duplicate_request_id(request_id))
    } else {
        Ok(())
    }
}

fn write_temporary_source(source_path: &Path, source: &[u8]) -> Result<PathBuf, AccountQueueError> {
    let parent = source_path.parent().ok_or_else(|| {
        AccountQueueError::temporary_write(
            "SavedVariables source has no parent directory".to_owned(),
        )
    })?;
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            AccountQueueError::temporary_write("SavedVariables source has no file name".to_owned())
        })?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| AccountQueueError::temporary_write(error.to_string()))?
        .as_nanos();

    for attempt in 0..10_000_u32 {
        let path = parent.join(format!(
            ".{file_name}.rpengine-manager-{}-{stamp}-{attempt}.tmp",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let write_result = file.write_all(source).and_then(|()| file.sync_all());
                if let Err(error) = write_result {
                    drop(file);
                    remove_temporary_file(&path);
                    return Err(AccountQueueError::temporary_write(error.to_string()));
                }
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(AccountQueueError::temporary_write(error.to_string())),
        }
    }
    Err(AccountQueueError::temporary_write(
        "could not create a unique temporary SavedVariables file".to_owned(),
    ))
}

fn validate_temporary_file(path: &Path, expected: &ProtocolState) -> Result<(), AccountQueueError> {
    let source = fs::read_to_string(path)
        .map_err(|error| AccountQueueError::temporary_validation(error.to_string()))?;
    let actual = parse_manager_state(&source)
        .map_err(|error| AccountQueueError::temporary_validation(error.to_string()))?;
    match actual {
        ManagerSavedVariables::Present(state) if state == *expected => Ok(()),
        ManagerSavedVariables::Present(_) => Err(AccountQueueError::temporary_validation(
            "temporary SavedVariables state differs from the queued operation".to_owned(),
        )),
        ManagerSavedVariables::Absent => Err(AccountQueueError::temporary_validation(
            "temporary SavedVariables file does not contain RPEngineManagerDB".to_owned(),
        )),
    }
}

fn remove_temporary_file(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(windows)]
fn replace_file_atomically(temporary_path: &Path, original_path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "Kernel32")]
    extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
    }

    const REPLACEFILE_WRITE_THROUGH: u32 = 0x0000_0001;
    let original: Vec<u16> = original_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let temporary: Vec<u16> = temporary_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // `temporary_path` is created beside `original_path`, so ReplaceFileW uses
    // one filesystem and replaces the existing live file in one OS operation.
    let replaced = unsafe {
        ReplaceFileW(
            original.as_ptr(),
            temporary.as_ptr(),
            std::ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file_atomically(temporary_path: &Path, original_path: &Path) -> io::Result<()> {
    fs::rename(temporary_path, original_path)
}

impl AccountQueueError {
    fn wow_running(safety: &WowModificationSafetyState) -> Self {
        let processes = if safety.matching_process_names.is_empty() {
            "World of Warcraft is running".to_owned()
        } else {
            format!(
                "World of Warcraft is running ({})",
                safety.matching_process_names.join(", ")
            )
        };
        Self {
            code: AccountQueueErrorCode::WowRunning,
            message: format!("{processes}; no SavedVariables changes were made."),
        }
    }

    fn source_read(path: &Path, error: io::Error) -> Self {
        Self {
            code: AccountQueueErrorCode::SourceRead,
            message: format!("Could not read {}: {error}", path.display()),
        }
    }

    fn source_not_file(path: &Path) -> Self {
        Self {
            code: AccountQueueErrorCode::SourceNotFile,
            message: format!("SavedVariables source {} is not a file", path.display()),
        }
    }

    fn parse(error: SavedVariablesError) -> Self {
        Self {
            code: AccountQueueErrorCode::Parse,
            message: error.to_string(),
        }
    }

    fn duplicate_request_id(request_id: &str) -> Self {
        Self {
            code: AccountQueueErrorCode::DuplicateRequestId,
            message: format!(
                "Request ID {request_id} already exists in pendingOperations or operationResults."
            ),
        }
    }

    fn backup(error: impl fmt::Display) -> Self {
        Self {
            code: AccountQueueErrorCode::Backup,
            message: format!("Backup failed; SavedVariables was left unchanged: {error}"),
        }
    }

    fn temporary_write(message: String) -> Self {
        Self {
            code: AccountQueueErrorCode::TemporaryWrite,
            message: format!("Could not write temporary SavedVariables replacement: {message}"),
        }
    }

    fn temporary_validation(message: String) -> Self {
        Self {
            code: AccountQueueErrorCode::TemporaryValidation,
            message: format!("Temporary SavedVariables validation failed: {message}"),
        }
    }

    fn atomic_replacement(error: io::Error) -> Self {
        Self {
            code: AccountQueueErrorCode::AtomicReplacement,
            message: format!("Could not replace SavedVariables atomically: {error}"),
        }
    }
}

#[derive(Debug)]
pub enum QueueOperationError {
    InvalidOperation(ProtocolValidationError),
    TargetResolution(SavedVariablesTargetError),
}

impl fmt::Display for QueueOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOperation(error) => write!(formatter, "Invalid queued operation: {error}"),
            Self::TargetResolution(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for QueueOperationError {}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::{
        configuration::{InstallationAvailability, WowInstallation, WowProduct},
        filesystem::{account_targets::SAVED_VARIABLES_FILE_NAME, backup::BackupStore},
        processes::wow::safety_state_from_process_names,
    };

    const PAYLOAD: &str = "RPE_DATASET_V1\nfixture payload\n";
    const HASH: &str = "2aee76c57aa954b6ca769bb1c1ff839c0805cea4aa4ffb3ebe153fe48e3c71de";

    struct Fixture {
        directory: PathBuf,
        configuration: ManagerConfiguration,
        backup_store: BackupStore,
    }

    impl Fixture {
        fn source_path(&self, account_id: &str) -> PathBuf {
            self.directory
                .join("_retail_")
                .join("WTF")
                .join("Account")
                .join(account_id)
                .join("SavedVariables")
                .join(SAVED_VARIABLES_FILE_NAME)
        }
    }

    fn fixture(name: &str, accounts: &[(&str, &str)]) -> Fixture {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("rpengine-manager-queue-{name}-{unique}"));
        let installation_path = directory.join("_retail_");
        fs::create_dir_all(installation_path.join("Data")).expect("create WoW data directory");
        fs::write(installation_path.join("Wow.exe"), "fixture executable")
            .expect("create WoW executable");
        for (account_id, source) in accounts {
            let source_path = installation_path
                .join("WTF")
                .join("Account")
                .join(account_id)
                .join("SavedVariables")
                .join(SAVED_VARIABLES_FILE_NAME);
            fs::create_dir_all(source_path.parent().unwrap())
                .expect("create SavedVariables directory");
            fs::write(source_path, source).expect("write SavedVariables source");
        }
        let installation = WowInstallation {
            id: "retail".to_owned(),
            product: Some(WowProduct::Retail),
            path: installation_path,
            availability: InstallationAvailability::Available,
        };
        let configuration = ManagerConfiguration {
            installations: vec![installation],
            selected_installation_id: Some("retail".to_owned()),
            selected_account_ids: [(
                "retail".to_owned(),
                accounts.iter().map(|(id, _)| (*id).to_owned()).collect(),
            )]
            .into_iter()
            .collect(),
            ..ManagerConfiguration::default()
        };
        let backup_store = BackupStore::new(directory.join("backups"));
        Fixture {
            directory,
            configuration,
            backup_store,
        }
    }

    fn install_request(request_id: &str) -> QueueInstallDatasetRequest {
        QueueInstallDatasetRequest {
            request_id: request_id.to_owned(),
            catalogue_id: "esarus-core".to_owned(),
            dataset_id: "f82db71a".to_owned(),
            revision: 14,
            hash: HASH.to_owned(),
            payload: PAYLOAD.to_owned(),
        }
    }

    fn remove_request(request_id: &str) -> QueueRemoveDatasetRequest {
        QueueRemoveDatasetRequest {
            request_id: request_id.to_owned(),
            catalogue_id: "esarus-core".to_owned(),
            dataset_id: "f82db71a".to_owned(),
            revision: 14,
            hash: HASH.to_owned(),
        }
    }

    fn ruleset_request(request_id: &str) -> QueueInstallRulesetRequest {
        QueueInstallRulesetRequest {
            request_id: request_id.to_owned(),
            payload: "RPE_RULESET_V1\n{ opaque = true }".to_owned(),
        }
    }

    fn safe_state() -> WowModificationSafetyState {
        safety_state_from_process_names(["explorer.exe"])
    }

    #[test]
    fn queues_an_install_and_preserves_unrelated_savedvariables_content() {
        let original =
            "RPEngineProfilesDB = { keep = \"exact\" }\nRPEngineDatasetDB = { protected = true }\n";
        let fixture = fixture("install", &[("ACCOUNT_A", original)]);

        let report = queue_install_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            safe_state,
            install_request("install-1"),
        )
        .expect("queue install");

        assert_eq!(report.accounts[0].status, AccountQueueStatus::Queued);
        let written =
            fs::read_to_string(fixture.source_path("ACCOUNT_A")).expect("read replacement");
        assert!(written.starts_with(original));
        let ManagerSavedVariables::Present(state) =
            parse_manager_state(&written).expect("parse queue")
        else {
            panic!("Manager root must be appended");
        };
        assert_eq!(state.pending_operations.len(), 1);
        assert_eq!(state.pending_operations[0].request_id, "install-1");
        let backup_id = report.accounts[0]
            .backup_id
            .as_ref()
            .expect("backup identity");
        assert_eq!(
            fs::read(
                fixture
                    .directory
                    .join("backups")
                    .join(backup_id)
                    .join("contents.bin")
            )
            .expect("read verified backup"),
            original.as_bytes()
        );
        assert_eq!(fixture.backup_store.list_backups().unwrap().len(), 1);
        fs::remove_dir_all(fixture.directory).expect("remove fixture");
    }

    #[test]
    fn queues_a_ruleset_without_writing_the_authored_ruleset_database() {
        let original = "RPEngineRulesetDB = { preserve = true }\n";
        let fixture = fixture("ruleset", &[("ACCOUNT_A", original), ("ACCOUNT_B", original)]);

        let report = queue_install_ruleset_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            safe_state,
            ruleset_request("ruleset-1"),
        )
        .expect("queue ruleset");

        assert!(report.accounts.iter().all(|account| account.status == AccountQueueStatus::Queued));
        for account_id in ["ACCOUNT_A", "ACCOUNT_B"] {
            let source = fs::read_to_string(fixture.source_path(account_id))
                .expect("read updated SavedVariables");
            assert!(source.contains("RPEngineRulesetDB = { preserve = true }"));
            let state = parse_manager_state(&source).expect("parse manager state");
            assert!(matches!(state, ManagerSavedVariables::Present(state)
                if state.pending_operations[0].operation == OperationKind::InstallRuleset));
        }
        fs::remove_dir_all(fixture.directory).expect("remove fixture");
    }

    #[test]
    fn rejects_empty_ruleset_before_creating_a_backup() {
        let fixture = fixture("empty-ruleset", &[("ACCOUNT_A", "")]);
        let report = queue_install_ruleset_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            safe_state,
            QueueInstallRulesetRequest { request_id: "ruleset-empty".to_owned(), payload: String::new() },
        );

        assert!(matches!(report, Err(QueueOperationError::InvalidOperation(_))));
        assert!(fixture.backup_store.list_backups().expect("list backups").is_empty());
        fs::remove_dir_all(fixture.directory).expect("remove fixture");
    }

    #[test]
    fn queues_a_remove_request() {
        let fixture = fixture("remove", &[("ACCOUNT_A", "RPEngineProfilesDB = {}\n")]);

        let report = queue_remove_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            safe_state,
            remove_request("remove-1"),
        )
        .expect("queue remove");

        assert_eq!(report.operation.operation, OperationKind::RemoveDataset);
        let written = fs::read_to_string(fixture.source_path("ACCOUNT_A")).unwrap();
        let ManagerSavedVariables::Present(state) = parse_manager_state(&written).unwrap() else {
            panic!("Manager root must be appended");
        };
        assert_eq!(
            state.pending_operations[0].operation,
            OperationKind::RemoveDataset
        );
        assert_eq!(state.pending_operations[0].payload, None);
        fs::remove_dir_all(fixture.directory).expect("remove fixture");
    }

    #[test]
    fn rejects_duplicate_request_ids_without_another_backup_or_write() {
        let fixture = fixture("duplicate", &[("ACCOUNT_A", "RPEngineProfilesDB = {}\n")]);
        queue_install_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            safe_state,
            install_request("duplicate-1"),
        )
        .expect("queue first install");
        let original_after_first = fs::read_to_string(fixture.source_path("ACCOUNT_A")).unwrap();

        let report = queue_install_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            safe_state,
            install_request("duplicate-1"),
        )
        .expect("report duplicate");

        assert_eq!(report.accounts[0].status, AccountQueueStatus::Failed);
        assert_eq!(
            report.accounts[0].error.as_ref().unwrap().code,
            AccountQueueErrorCode::DuplicateRequestId
        );
        assert_eq!(
            fs::read_to_string(fixture.source_path("ACCOUNT_A")).unwrap(),
            original_after_first
        );
        assert_eq!(fixture.backup_store.list_backups().unwrap().len(), 1);

        let completed = "RPEngineManagerDB = {\n    protocolVersion = 1,\n    pendingOperations = {},\n    installedPackages = {},\n    operationResults = {\n        [\"completed-1\"] = {\n            requestId = \"completed-1\",\n            operation = \"install_dataset\",\n            status = \"failed\",\n            error = {\n                code = \"invalid_field\",\n                detail = \"already completed\",\n            },\n        },\n    },\n}\n";
        fs::write(fixture.source_path("ACCOUNT_A"), completed).unwrap();
        let report = queue_install_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            safe_state,
            install_request("completed-1"),
        )
        .expect("report completed request ID");
        assert_eq!(
            report.accounts[0].error.as_ref().unwrap().code,
            AccountQueueErrorCode::DuplicateRequestId
        );
        assert_eq!(
            fs::read_to_string(fixture.source_path("ACCOUNT_A")).unwrap(),
            completed
        );
        assert_eq!(fixture.backup_store.list_backups().unwrap().len(), 1);
        fs::remove_dir_all(fixture.directory).expect("remove fixture");
    }

    #[test]
    fn mismatched_install_hash_is_rejected_before_any_account_is_touched() {
        let original = "RPEngineProfilesDB = {}\n";
        let fixture = fixture("hash", &[("ACCOUNT_A", original)]);
        let mut request = install_request("bad-hash-1");
        request.hash = "0".repeat(64);

        assert!(matches!(
            queue_install_for_selected_accounts(
                &fixture.configuration,
                &fixture.backup_store,
                safe_state,
                request,
            ),
            Err(QueueOperationError::InvalidOperation(_))
        ));
        assert_eq!(
            fs::read_to_string(fixture.source_path("ACCOUNT_A")).unwrap(),
            original
        );
        assert!(fixture.backup_store.list_backups().unwrap().is_empty());
        fs::remove_dir_all(fixture.directory).expect("remove fixture");
    }

    #[test]
    fn malformed_state_and_backup_failure_leave_the_source_unchanged() {
        let malformed = "RPEngineManagerDB = { protocolVersion = 1\n";
        let fixture = fixture("malformed", &[("ACCOUNT_A", malformed)]);
        let report = queue_install_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            safe_state,
            install_request("malformed-1"),
        )
        .expect("report malformed state");
        assert_eq!(
            report.accounts[0].error.as_ref().unwrap().code,
            AccountQueueErrorCode::Parse
        );
        assert_eq!(
            fs::read_to_string(fixture.source_path("ACCOUNT_A")).unwrap(),
            malformed
        );

        let backup_root_file = fixture.directory.join("backup-root-file");
        fs::write(&backup_root_file, "not a directory").expect("create blocking backup root");
        let failing_backups = BackupStore::new(backup_root_file);
        let valid_source = "RPEngineProfilesDB = {}\n";
        fs::write(fixture.source_path("ACCOUNT_A"), valid_source).unwrap();
        let report = queue_install_for_selected_accounts(
            &fixture.configuration,
            &failing_backups,
            safe_state,
            install_request("backup-failure-1"),
        )
        .expect("report backup failure");
        assert_eq!(
            report.accounts[0].error.as_ref().unwrap().code,
            AccountQueueErrorCode::Backup
        );
        assert_eq!(
            fs::read_to_string(fixture.source_path("ACCOUNT_A")).unwrap(),
            valid_source
        );
        fs::remove_dir_all(fixture.directory).expect("remove fixture");
    }

    #[test]
    fn temporary_validation_failure_leaves_the_original_file_intact() {
        let original = "RPEngineProfilesDB = {}\n";
        let fixture = fixture("temporary-validation", &[("ACCOUNT_A", original)]);
        let target = resolve_selected_saved_variables_targets(&fixture.configuration)
            .unwrap()
            .remove(0);
        let operation = install_request("temporary-validation-1").into_operation();

        let error = queue_one_account(&target, &operation, &fixture.backup_store, |_, _| {
            Err(AccountQueueError::temporary_validation(
                "simulated failure".to_owned(),
            ))
        })
        .expect_err("temporary validation must prevent replacement");

        assert_eq!(error.code, AccountQueueErrorCode::TemporaryValidation);
        assert_eq!(
            fs::read_to_string(fixture.source_path("ACCOUNT_A")).unwrap(),
            original
        );
        assert_eq!(fixture.backup_store.list_backups().unwrap().len(), 1);
        fs::remove_dir_all(fixture.directory).expect("remove fixture");
    }

    #[test]
    fn wow_running_blocks_every_account_and_partial_failures_are_reported() {
        let valid = "RPEngineProfilesDB = {}\n";
        let malformed = "RPEngineManagerDB = { protocolVersion = 1\n";
        let fixture = fixture(
            "multi-account",
            &[("ACCOUNT_A", valid), ("ACCOUNT_B", malformed)],
        );

        let running = queue_install_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            || safety_state_from_process_names(["Wow.exe"]),
            install_request("blocked-1"),
        )
        .expect("report running game");
        assert!(running.accounts.iter().all(
            |account| account.error.as_ref().unwrap().code == AccountQueueErrorCode::WowRunning
        ));
        assert_eq!(fixture.backup_store.list_backups().unwrap().len(), 0);

        let partial = queue_install_for_selected_accounts(
            &fixture.configuration,
            &fixture.backup_store,
            safe_state,
            install_request("partial-1"),
        )
        .expect("report partial result");
        assert_eq!(partial.accounts[0].status, AccountQueueStatus::Queued);
        assert_eq!(partial.accounts[1].status, AccountQueueStatus::Failed);
        assert_eq!(
            partial.accounts[1].error.as_ref().unwrap().code,
            AccountQueueErrorCode::Parse
        );
        fs::remove_dir_all(fixture.directory).expect("remove fixture");
    }
}
