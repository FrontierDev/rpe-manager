//! Tauri entry point for the single RPEngine install/update/repair transaction.

use std::{fmt, fs, path::Path};

use serde::Serialize;
use tauri::Manager;

#[cfg(windows)]
use crate::addon_transaction::replace_staged_addon_privileged;
#[cfg(not(windows))]
use crate::{addon_transaction::replace_staged_addon, processes::wow::inspect_wow_processes};
use crate::{
    addon_transaction::{AddonTransactionError, AddonTransactionReport},
    commands::configuration::configuration_store,
    configuration::{ConfigurationCommandError, WowInstallation},
    discovery::rpengine::{discover_rpengine, RPEngineInstallation, RPEngineInstallationStatus},
    release::{download_and_stage_release, fetch_latest_release, ReleaseError, RpeVersion},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpeUpdateState {
    pub local: RPEngineInstallation,
    pub status: RpeUpdateStatus,
    pub latest_version: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RpeUpdateStatus {
    NotInstalled,
    Current,
    UpdateAvailable,
    Damaged,
    VersionUnavailable,
    Unsupported,
    CheckFailed,
}

#[tauri::command]
pub fn get_rpengine_update_state(
    app: tauri::AppHandle,
) -> Result<RpeUpdateState, ReleaseCommandError> {
    let installation = selected_installation(&app)?;
    let local = discover_rpengine(&installation.path)
        .map_err(|error| ReleaseCommandError::release_message(error.to_string()))?;
    match fetch_latest_release() {
        Ok(release) => Ok(classify_update_state(local, Some(release.tag_name), None)),
        Err(message) => Ok(classify_update_state(local, None, Some(message))),
    }
}

pub fn classify_update_state(
    local: RPEngineInstallation,
    latest_version: Option<String>,
    check_error: Option<String>,
) -> RpeUpdateState {
    if let Some(detail) = check_error {
        return RpeUpdateState {
            local,
            status: RpeUpdateStatus::CheckFailed,
            latest_version: None,
            detail: Some(detail),
        };
    }
    let status = match local.status {
        RPEngineInstallationStatus::NotInstalled => RpeUpdateStatus::NotInstalled,
        RPEngineInstallationStatus::Damaged => RpeUpdateStatus::Damaged,
        RPEngineInstallationStatus::VersionUnavailable => RpeUpdateStatus::VersionUnavailable,
        RPEngineInstallationStatus::Unsupported => RpeUpdateStatus::Unsupported,
        RPEngineInstallationStatus::Installed => match (
            local
                .version
                .as_deref()
                .and_then(|value| RpeVersion::parse(value).ok()),
            latest_version
                .as_deref()
                .and_then(|value| RpeVersion::parse(value).ok()),
        ) {
            (Some(installed), Some(latest)) if installed < latest => {
                RpeUpdateStatus::UpdateAvailable
            }
            (Some(_), Some(_)) => RpeUpdateStatus::Current,
            _ => RpeUpdateStatus::Unsupported,
        },
    };
    RpeUpdateState {
        local,
        status,
        latest_version,
        detail: None,
    }
}

#[tauri::command]
pub fn install_latest_rpengine(
    app: tauri::AppHandle,
) -> Result<AddonTransactionReport, ReleaseCommandError> {
    let installation = selected_installation(&app)?;
    let staging_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| ReleaseCommandError::storage(error.to_string()))?
        .join("release-staging");
    fs::create_dir_all(&staging_root)
        .map_err(|error| ReleaseCommandError::storage(error.to_string()))?;
    cleanup_stale_staging(&staging_root).map_err(ReleaseCommandError::storage)?;

    let operation_id = crate::elevation::operation_id();
    let operation_directory = staging_root.join(format!("rpengine-op-{operation_id}"));
    fs::create_dir(&operation_directory)
        .map_err(|error| ReleaseCommandError::storage(error.to_string()))?;
    let operation_staging = operation_directory.join("staged");
    if let Err(error) = fs::create_dir(&operation_staging) {
        let cleanup = fs::remove_dir_all(&operation_directory);
        return Err(ReleaseCommandError::storage(match cleanup {
            Ok(()) => error.to_string(),
            Err(cleanup_error) => format!(
                "Could not create operation staging: {error}; cleanup also failed: {cleanup_error}"
            ),
        }));
    }

    let result = (|| {
        let release = fetch_latest_release().map_err(ReleaseCommandError::release_message)?;
        let staged = download_and_stage_release(&release, &operation_staging)
            .map_err(ReleaseCommandError::release)?;

        #[cfg(windows)]
        let transaction = if crate::elevation::process_is_elevated() {
            replace_staged_addon_privileged(&installation.path, &staged)
                .map_err(ReleaseCommandError::transaction)
        } else {
            crate::elevation::replace_elevated(&installation.path, &staged)
                .map_err(ReleaseCommandError::elevation)
        };

        #[cfg(not(windows))]
        let transaction =
            replace_staged_addon(&installation.path, &staged, inspect_wow_processes())
                .map_err(ReleaseCommandError::transaction);

        transaction
    })();
    let cleanup = fs::remove_dir_all(&operation_directory);
    let result = match (result, cleanup) {
        (Ok(report), Ok(())) => Ok(report),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(ReleaseCommandError::storage(format!(
            "RPEngine replacement completed, but Manager operation storage could not be cleaned: {error}"
        ))),
        (Err(mut error), Err(cleanup_error)) => {
            error.message.push_str(&format!(
                " Manager operation storage cleanup also failed: {cleanup_error}"
            ));
            Err(error)
        }
    };
    result
}

/// Crash leftovers are safe to remove only when they are direct children of
/// the dedicated Manager cache directory and carry our operation prefix.
fn cleanup_stale_staging(root: &Path) -> Result<(), String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("Could not resolve Manager staging directory: {error}"))?;
    let running_processes = sysinfo::System::new_all();
    for entry in fs::read_dir(&canonical_root)
        .map_err(|error| format!("Could not inspect Manager staging directory: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("Could not inspect staged operation: {error}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let owner = name
            .strip_prefix("rpengine-op-")
            .or_else(|| name.strip_prefix("rpengine-stage-"))
            .and_then(|suffix| suffix.split_once('-'))
            .and_then(|(pid, nonce)| Some((pid.parse::<u32>().ok()?, nonce.parse::<u128>().ok()?)));
        let kind = entry
            .file_type()
            .map_err(|error| format!("Could not inspect {}: {error}", entry.path().display()))?;
        let owner_is_running = owner.is_some_and(|(owner_pid, _)| {
            running_processes
                .processes()
                .keys()
                .any(|pid| pid.as_u32() == owner_pid)
        });
        if owner.is_some() && !owner_is_running && kind.is_dir() && !kind.is_symlink() {
            let canonical = entry.path().canonicalize().map_err(|error| {
                format!("Could not resolve {}: {error}", entry.path().display())
            })?;
            if canonical.parent() == Some(canonical_root.as_path()) {
                fs::remove_dir_all(canonical).map_err(|error| {
                    format!("Could not remove stale Manager operation storage: {error}")
                })?;
            }
        }
    }
    Ok(())
}

fn selected_installation(app: &tauri::AppHandle) -> Result<WowInstallation, ReleaseCommandError> {
    let configuration = configuration_store(app)
        .map_err(ReleaseCommandError::configuration)?
        .load()
        .map_err(|error| ReleaseCommandError::configuration(error.into()))?
        .configuration;
    let selected = configuration
        .selected_installation_id
        .ok_or_else(ReleaseCommandError::no_selected_installation)?;
    configuration
        .installations
        .into_iter()
        .find(|installation| installation.id == selected)
        .ok_or_else(|| {
            ReleaseCommandError::configuration_message(format!(
                "Selected installation {selected} no longer exists in configuration."
            ))
        })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseCommandError {
    pub code: ReleaseCommandErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseCommandErrorCode {
    Configuration,
    NoSelectedInstallation,
    Release,
    Storage,
    Transaction,
    Elevation,
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    fn local(status: RPEngineInstallationStatus, version: Option<&str>) -> RPEngineInstallation {
        RPEngineInstallation {
            status,
            version: version.map(str::to_owned),
            detail: None,
        }
    }
    #[test]
    fn classifies_current_updates_and_check_failures_without_losing_local_state() {
        assert_eq!(
            classify_update_state(
                local(RPEngineInstallationStatus::Installed, Some("2.0.alpha5")),
                Some("2.0.alpha5".to_owned()),
                None
            )
            .status,
            RpeUpdateStatus::Current
        );
        assert_eq!(
            classify_update_state(
                local(RPEngineInstallationStatus::Installed, Some("2.0.alpha4")),
                Some("2.0.alpha5".to_owned()),
                None
            )
            .status,
            RpeUpdateStatus::UpdateAvailable
        );
        let state = classify_update_state(
            local(RPEngineInstallationStatus::Installed, Some("2.0.alpha5")),
            None,
            Some("offline".to_owned()),
        );
        assert_eq!(state.status, RpeUpdateStatus::CheckFailed);
        assert_eq!(state.local.version.as_deref(), Some("2.0.alpha5"));
    }

    #[test]
    fn stale_cleanup_removes_only_dead_manager_operation_directories() {
        let root = std::env::temp_dir().join(format!(
            "rpengine-stale-cleanup-{}",
            crate::elevation::operation_id()
        ));
        let dead_operation = root.join("rpengine-op-4294967295-1");
        let unrelated = root.join("rpengine-op-not-a-manager-id");
        fs::create_dir_all(dead_operation.join("staged")).expect("create stale operation");
        fs::create_dir_all(&unrelated).expect("create unrelated directory");

        cleanup_stale_staging(&root).expect("clean stale staging");
        assert!(!dead_operation.exists());
        assert!(unrelated.is_dir());
        fs::remove_dir_all(root).expect("remove fixture");
    }
}

impl ReleaseCommandError {
    fn configuration(error: ConfigurationCommandError) -> Self {
        Self::configuration_message(error.message)
    }
    fn configuration_message(message: String) -> Self {
        Self {
            code: ReleaseCommandErrorCode::Configuration,
            message,
        }
    }
    fn no_selected_installation() -> Self {
        Self {
            code: ReleaseCommandErrorCode::NoSelectedInstallation,
            message: "No World of Warcraft installation is selected.".to_owned(),
        }
    }
    fn release(error: ReleaseError) -> Self {
        Self::release_message(error.to_string())
    }
    fn release_message(message: String) -> Self {
        Self {
            code: ReleaseCommandErrorCode::Release,
            message,
        }
    }
    fn storage(message: String) -> Self {
        Self {
            code: ReleaseCommandErrorCode::Storage,
            message,
        }
    }
    fn transaction(error: AddonTransactionError) -> Self {
        Self {
            code: ReleaseCommandErrorCode::Transaction,
            message: error.to_string(),
        }
    }
    fn elevation(message: String) -> Self {
        Self {
            code: ReleaseCommandErrorCode::Elevation,
            message,
        }
    }
}
impl fmt::Display for ReleaseCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for ReleaseCommandError {}
