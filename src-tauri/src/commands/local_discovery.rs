//! Commands that inspect only the installation selected in Manager configuration.

use std::fmt;

use serde::Serialize;

use crate::{
    commands::configuration::configuration_store,
    configuration::{ConfigurationCommandError, WowInstallation},
    discovery::{
        accounts::{discover_accounts, AccountDiscoveryError, WowAccount},
        rpengine::{discover_rpengine, RPEngineDiscoveryError, RPEngineInstallation},
    },
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedWowInstallationDiscovery {
    pub installation: WowInstallation,
    pub accounts: Vec<WowAccount>,
    pub rpengine: RPEngineInstallation,
}

#[tauri::command]
pub fn discover_selected_wow_installation(
    app: tauri::AppHandle,
) -> Result<SelectedWowInstallationDiscovery, LocalDiscoveryCommandError> {
    let configuration = configuration_store(&app)
        .map_err(LocalDiscoveryCommandError::configuration)?
        .load()
        .map_err(|error| LocalDiscoveryCommandError::configuration(error.into()))?
        .configuration;
    let selected_id = configuration
        .selected_installation_id
        .clone()
        .ok_or_else(LocalDiscoveryCommandError::no_selected_installation)?;
    let installation = configuration
        .installations
        .into_iter()
        .find(|installation| installation.id == selected_id)
        .ok_or_else(|| {
            LocalDiscoveryCommandError::configuration_message(format!(
                "Selected installation {selected_id} no longer exists in configuration."
            ))
        })?;
    let accounts =
        discover_accounts(&installation.path).map_err(LocalDiscoveryCommandError::accounts)?;
    let rpengine =
        discover_rpengine(&installation.path).map_err(LocalDiscoveryCommandError::rpengine)?;

    Ok(SelectedWowInstallationDiscovery {
        installation,
        accounts,
        rpengine,
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDiscoveryCommandError {
    pub code: LocalDiscoveryCommandErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalDiscoveryCommandErrorCode {
    Accounts,
    Configuration,
    NoSelectedInstallation,
    RPEngine,
}

impl LocalDiscoveryCommandError {
    fn configuration(error: ConfigurationCommandError) -> Self {
        Self::configuration_message(error.message)
    }

    fn configuration_message(message: String) -> Self {
        Self {
            code: LocalDiscoveryCommandErrorCode::Configuration,
            message,
        }
    }

    fn no_selected_installation() -> Self {
        Self {
            code: LocalDiscoveryCommandErrorCode::NoSelectedInstallation,
            message: "No World of Warcraft installation is selected.".to_owned(),
        }
    }

    pub(crate) fn accounts(error: AccountDiscoveryError) -> Self {
        Self {
            code: LocalDiscoveryCommandErrorCode::Accounts,
            message: error.to_string(),
        }
    }

    pub(crate) fn rpengine(error: RPEngineDiscoveryError) -> Self {
        Self {
            code: LocalDiscoveryCommandErrorCode::RPEngine,
            message: error.to_string(),
        }
    }
}

impl fmt::Display for LocalDiscoveryCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LocalDiscoveryCommandError {}
