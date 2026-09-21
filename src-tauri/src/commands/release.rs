//! Tauri entry point for the single RPEngine install/update/repair transaction.

use std::{fmt, fs};

use serde::Serialize;
use tauri::Manager;

use crate::{
    addon_transaction::{replace_staged_addon, AddonTransactionError, AddonTransactionReport},
    commands::configuration::configuration_store,
    configuration::{ConfigurationCommandError, WowInstallation},
    discovery::rpengine::{discover_rpengine, RPEngineInstallation, RPEngineInstallationStatus},
    processes::wow::inspect_wow_processes,
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
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| ReleaseCommandError::storage(error.to_string()))?
        .join("release-staging");
    let release = fetch_latest_release().map_err(ReleaseCommandError::release_message)?;
    let staged =
        download_and_stage_release(&release, &cache).map_err(ReleaseCommandError::release)?;
    let result = replace_staged_addon(&installation.path, &staged, inspect_wow_processes())
        .map_err(ReleaseCommandError::transaction);
    let _ = fs::remove_dir_all(&staged.directory);
    result
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
}
impl fmt::Display for ReleaseCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for ReleaseCommandError {}
