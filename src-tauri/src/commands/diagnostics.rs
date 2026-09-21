//! Read-only Phase 1 diagnostic snapshot for the Manager UI.

use serde::Serialize;
use sysinfo::System;

use crate::{
    commands::{
        configuration::configuration_store,
        local_discovery::{LocalDiscoveryCommandError, SelectedWowInstallationDiscovery},
    },
    configuration::{ConfigurationCommandError, WowInstallation},
    discovery::wow::{discover, DiscoveryInputs, WowInstallationCandidate},
    processes::wow::{inspect_wow_processes, WowModificationSafetyState},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseOneDiagnostics {
    pub manager_version: String,
    pub operating_system: String,
    pub detected_installations: Vec<WowInstallationCandidate>,
    pub selected_installation: Option<WowInstallation>,
    pub selected_installation_discovery: Option<SelectedWowInstallationDiscovery>,
    pub wow_safety: WowModificationSafetyState,
}

#[tauri::command]
pub fn get_phase_one_diagnostics(
    app: tauri::AppHandle,
) -> Result<PhaseOneDiagnostics, DiagnosticsCommandError> {
    let configuration = configuration_store(&app)
        .map_err(DiagnosticsCommandError::configuration)?
        .load()
        .map_err(|error| DiagnosticsCommandError::configuration(error.into()))?
        .configuration;
    let detected_installations = discover(&DiscoveryInputs::from_current_environment(
        configuration.installations.clone(),
    ));
    let selected_installation =
        configuration
            .selected_installation_id
            .as_ref()
            .and_then(|selected_id| {
                configuration
                    .installations
                    .iter()
                    .find(|installation| installation.id == *selected_id)
                    .cloned()
            });
    let selected_installation_discovery = match selected_installation.as_ref() {
        Some(installation)
            if installation.availability
                == crate::configuration::InstallationAvailability::Available =>
        {
            Some(
                selected_installation_details(installation)
                    .map_err(DiagnosticsCommandError::discovery)?,
            )
        }
        _ => None,
    };

    Ok(PhaseOneDiagnostics {
        manager_version: env!("CARGO_PKG_VERSION").to_owned(),
        operating_system: System::long_os_version()
            .unwrap_or_else(|| std::env::consts::OS.to_owned()),
        detected_installations,
        selected_installation,
        selected_installation_discovery,
        wow_safety: inspect_wow_processes(),
    })
}

fn selected_installation_details(
    installation: &WowInstallation,
) -> Result<SelectedWowInstallationDiscovery, LocalDiscoveryCommandError> {
    use crate::discovery::{accounts::discover_accounts, rpengine::discover_rpengine};

    let accounts =
        discover_accounts(&installation.path).map_err(LocalDiscoveryCommandError::accounts)?;
    let rpengine =
        discover_rpengine(&installation.path).map_err(LocalDiscoveryCommandError::rpengine)?;
    Ok(SelectedWowInstallationDiscovery {
        installation: installation.clone(),
        accounts,
        rpengine,
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsCommandError {
    pub code: DiagnosticsCommandErrorCode,
    pub message: String,
}

impl DiagnosticsCommandError {
    fn configuration(error: ConfigurationCommandError) -> Self {
        Self {
            code: DiagnosticsCommandErrorCode::Configuration,
            message: error.message,
        }
    }

    fn discovery(error: LocalDiscoveryCommandError) -> Self {
        Self {
            code: DiagnosticsCommandErrorCode::Discovery,
            message: error.message,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticsCommandErrorCode {
    Configuration,
    Discovery,
}
