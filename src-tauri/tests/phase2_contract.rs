//! Contract-fixture validation for the Manager half of the Phase 2 boundary.
//!
//! The persisted post-consumption states are recorded protocol fixtures. This
//! test intentionally does not emulate RPE's importer or deleter; the manual
//! validation guide covers those canonical addon calls in WoW.

use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use rpengine_manager_lib::{
    configuration::{InstallationAvailability, ManagerConfiguration, WowInstallation, WowProduct},
    filesystem::{
        backup::BackupStore,
        operation_queue::{
            queue_install_for_selected_accounts, queue_remove_for_selected_accounts,
            AccountQueueErrorCode, AccountQueueStatus, QueueInstallDatasetRequest,
            QueueRemoveDatasetRequest,
        },
        protocol_state::{
            read_selected_protocol_state, reconcile_request, RequestReconciliationStatus,
        },
        saved_variables::{
            parse_manager_state, serialize_manager_assignment, ManagerSavedVariables,
        },
    },
    processes::wow::safety_state_from_process_names,
    protocol::{OperationEnvelope, ProtocolState},
};

const INSTALL_FIXTURE: &str = include_str!("../../docs/protocol-v1/valid-install-request.json");
const INSTALL_SUCCESS: &str = include_str!("../../docs/protocol-v1/successful-result.json");
const UPDATE_FIXTURE: &str = include_str!("../../docs/protocol-v1/valid-update-request.json");
const UPDATE_SUCCESS: &str = include_str!("../../docs/protocol-v1/update-successful-result.json");
const REMOVE_FIXTURE: &str = include_str!("../../docs/protocol-v1/valid-remove-request.json");
const REMOVE_SUCCESS: &str = include_str!("../../docs/protocol-v1/remove-successful-result.json");
const DATASET_ID_MISMATCH: &str =
    include_str!("../../docs/protocol-v1/failed-dataset-id-mismatch.json");
const REMOVAL_IDENTITY_MISMATCH: &str =
    include_str!("../../docs/protocol-v1/failed-removal-identity-mismatch.json");

struct Fixture {
    directory: PathBuf,
    configuration: ManagerConfiguration,
    backups: BackupStore,
}

impl Fixture {
    fn source_path(&self) -> PathBuf {
        self.directory
            .join("_retail_")
            .join("WTF")
            .join("Account")
            .join("ACCOUNT_PHASE2")
            .join("SavedVariables")
            .join("RPEngine2.lua")
    }
}

fn fixture() -> Fixture {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("rpengine-manager-phase2-{unique}"));
    let installation_path = directory.join("_retail_");
    fs::create_dir_all(installation_path.join("Data")).expect("create WoW data directory");
    fs::write(installation_path.join("Wow.exe"), "fixture executable")
        .expect("create WoW executable");
    let source_path = installation_path
        .join("WTF")
        .join("Account")
        .join("ACCOUNT_PHASE2")
        .join("SavedVariables")
        .join("RPEngine2.lua");
    fs::create_dir_all(source_path.parent().expect("SavedVariables parent"))
        .expect("create SavedVariables directory");
    fs::write(
        source_path,
        "RPEngineProfilesDB = { keep = true }\nRPEngineDatasetDB = { untouched = true }\n",
    )
    .expect("write SavedVariables source");

    Fixture {
        directory: directory.clone(),
        configuration: ManagerConfiguration {
            installations: vec![WowInstallation {
                id: "retail".to_owned(),
                product: Some(WowProduct::Retail),
                path: installation_path,
                availability: InstallationAvailability::Available,
            }],
            selected_installation_id: Some("retail".to_owned()),
            selected_account_ids: [("retail".to_owned(), vec!["ACCOUNT_PHASE2".to_owned()])]
                .into_iter()
                .collect(),
            ..ManagerConfiguration::default()
        },
        backups: BackupStore::new(directory.join("backups")),
    }
}

fn protocol_fixture(source: &str) -> ProtocolState {
    let state: ProtocolState = serde_json::from_str(source).expect("deserialize protocol fixture");
    state.validate().expect("validate protocol fixture");
    state
}

fn only_operation(state: &ProtocolState) -> OperationEnvelope {
    assert_eq!(state.pending_operations.len(), 1, "fixture operation count");
    state.pending_operations[0].clone()
}

fn install_request(operation: OperationEnvelope) -> QueueInstallDatasetRequest {
    QueueInstallDatasetRequest {
        request_id: operation.request_id,
        catalogue_id: operation.catalogue_id,
        dataset_id: operation.dataset_id,
        revision: operation.revision,
        hash: operation.hash,
        payload: operation.payload.expect("install fixture payload"),
    }
}

fn remove_request(operation: OperationEnvelope) -> QueueRemoveDatasetRequest {
    QueueRemoveDatasetRequest {
        request_id: operation.request_id,
        catalogue_id: operation.catalogue_id,
        dataset_id: operation.dataset_id,
        revision: operation.revision,
        hash: operation.hash,
    }
}

fn safe_state() -> rpengine_manager_lib::processes::wow::WowModificationSafetyState {
    safety_state_from_process_names(["explorer.exe"])
}

fn write_persisted_addon_state(fixture: &Fixture, state: &ProtocolState) {
    let unrelated =
        "RPEngineProfilesDB = { keep = true }\nRPEngineDatasetDB = { untouched = true }\n";
    fs::write(
        fixture.source_path(),
        format!(
            "{unrelated}{}\n",
            serialize_manager_assignment(state).unwrap()
        ),
    )
    .expect("record persisted addon fixture state");
}

#[test]
fn phase_two_contract_fixtures_cover_queue_update_removal_and_persisted_failures() {
    let fixture = fixture();
    let install = only_operation(&protocol_fixture(INSTALL_FIXTURE));

    let queued = queue_install_for_selected_accounts(
        &fixture.configuration,
        &fixture.backups,
        safe_state,
        install_request(install.clone()),
    )
    .expect("queue install fixture");
    assert_eq!(queued.accounts[0].status, AccountQueueStatus::Queued);
    let backup_id = queued.accounts[0]
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
        .expect("read backup"),
        b"RPEngineProfilesDB = { keep = true }\nRPEngineDatasetDB = { untouched = true }\n"
    );
    let ManagerSavedVariables::Present(queued_state) =
        parse_manager_state(&fs::read_to_string(fixture.source_path()).unwrap()).unwrap()
    else {
        panic!("queued Manager root must be persisted");
    };
    assert_eq!(queued_state.pending_operations, vec![install]);

    // This represents the state saved by RPE after its canonical
    // Database.ImportDataset path, not a Manager-side import implementation.
    write_persisted_addon_state(&fixture, &protocol_fixture(INSTALL_SUCCESS));
    let snapshot = read_selected_protocol_state(&fixture.configuration, &safe_state()).unwrap();
    assert_eq!(snapshot.accounts[0].installed_packages[0].revision, 14);
    assert_eq!(
        reconcile_request(&snapshot, "550e8400-e29b-41d4-a716-446655440000")
            .unwrap()
            .accounts[0]
            .status,
        RequestReconciliationStatus::Succeeded
    );

    let update = only_operation(&protocol_fixture(UPDATE_FIXTURE));
    let queued = queue_install_for_selected_accounts(
        &fixture.configuration,
        &fixture.backups,
        safe_state,
        install_request(update.clone()),
    )
    .expect("queue update fixture");
    assert_eq!(queued.accounts[0].status, AccountQueueStatus::Queued);
    write_persisted_addon_state(&fixture, &protocol_fixture(UPDATE_SUCCESS));
    let snapshot = read_selected_protocol_state(&fixture.configuration, &safe_state()).unwrap();
    assert_eq!(snapshot.accounts[0].installed_packages.len(), 1);
    assert_eq!(snapshot.accounts[0].installed_packages[0].revision, 15);
    assert_eq!(
        reconcile_request(&snapshot, &update.request_id)
            .unwrap()
            .accounts[0]
            .status,
        RequestReconciliationStatus::Succeeded
    );

    let remove = only_operation(&protocol_fixture(REMOVE_FIXTURE));
    let queued = queue_remove_for_selected_accounts(
        &fixture.configuration,
        &fixture.backups,
        safe_state,
        remove_request(remove.clone()),
    )
    .expect("queue removal fixture");
    assert_eq!(queued.accounts[0].status, AccountQueueStatus::Queued);
    // This represents the state saved after RPE's canonical
    // Database.DeleteDataset path has completed.
    write_persisted_addon_state(&fixture, &protocol_fixture(REMOVE_SUCCESS));
    let snapshot = read_selected_protocol_state(&fixture.configuration, &safe_state()).unwrap();
    assert!(snapshot.accounts[0].installed_packages.is_empty());
    assert_eq!(
        reconcile_request(&snapshot, &remove.request_id)
            .unwrap()
            .accounts[0]
            .status,
        RequestReconciliationStatus::Succeeded
    );

    write_persisted_addon_state(&fixture, &protocol_fixture(DATASET_ID_MISMATCH));
    let snapshot = read_selected_protocol_state(&fixture.configuration, &safe_state()).unwrap();
    let mismatch = reconcile_request(&snapshot, "550e8400-e29b-41d4-a716-446655440020").unwrap();
    assert_eq!(
        mismatch.accounts[0].status,
        RequestReconciliationStatus::Failed
    );
    assert_eq!(
        mismatch.accounts[0]
            .result
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .code,
        rpengine_manager_lib::protocol::ProtocolErrorCode::DatasetIdMismatch
    );

    write_persisted_addon_state(&fixture, &protocol_fixture(REMOVAL_IDENTITY_MISMATCH));
    let snapshot = read_selected_protocol_state(&fixture.configuration, &safe_state()).unwrap();
    assert_eq!(snapshot.accounts[0].installed_packages.len(), 1);
    let mismatch = reconcile_request(&snapshot, "550e8400-e29b-41d4-a716-446655440021").unwrap();
    assert_eq!(
        mismatch.accounts[0].status,
        RequestReconciliationStatus::Failed
    );
    assert_eq!(
        mismatch.accounts[0]
            .result
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .code,
        rpengine_manager_lib::protocol::ProtocolErrorCode::InstalledPackageMismatch
    );

    write_persisted_addon_state(&fixture, &protocol_fixture(INSTALL_SUCCESS));
    let duplicate = queue_install_for_selected_accounts(
        &fixture.configuration,
        &fixture.backups,
        safe_state,
        install_request(only_operation(&protocol_fixture(INSTALL_FIXTURE))),
    )
    .expect("report duplicate completed request");
    assert_eq!(duplicate.accounts[0].status, AccountQueueStatus::Failed);
    assert_eq!(
        duplicate.accounts[0].error.as_ref().unwrap().code,
        AccountQueueErrorCode::DuplicateRequestId
    );

    fs::remove_dir_all(fixture.directory).expect("remove fixture directory");
}
