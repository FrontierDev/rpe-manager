//! Tauri commands for read-only WoW discovery and explicit installation choice.

use std::{fmt, fs, path::PathBuf};

use serde::Serialize;

use crate::{
    commands::configuration::configuration_store,
    configuration::{
        ConfigurationCommandError, ConfigurationValidationError, InstallationAvailability,
        ManagerConfiguration, WowInstallation,
    },
    discovery::wow::{
        discover, installation_id, validate_installation_path, DiscoveryInputs,
        ValidatedWowInstallation, WowInstallationCandidate, WowValidationError,
    },
};

#[tauri::command]
pub fn discover_wow_installations(
    app: tauri::AppHandle,
) -> Result<Vec<WowInstallationCandidate>, WowDiscoveryCommandError> {
    let configuration = configuration_store(&app)
        .map_err(WowDiscoveryCommandError::configuration)?
        .load()
        .map_err(|error| WowDiscoveryCommandError::configuration(error.into()))?
        .configuration;

    Ok(discover(&DiscoveryInputs::from_current_environment(
        configuration.installations,
    )))
}

/// Validates a folder chosen through the native dialog, then stores and selects
/// only that exact installation. It never changes files under the WoW folder.
#[tauri::command]
pub fn select_wow_installation(
    app: tauri::AppHandle,
    path: PathBuf,
) -> Result<ManagerConfiguration, WowDiscoveryCommandError> {
    let validated =
        validate_installation_path(&path).map_err(WowDiscoveryCommandError::invalid_path)?;
    let canonical_path = fs::canonicalize(&validated.path).map_err(|source| {
        WowDiscoveryCommandError::invalid_path(WowValidationError::InspectPath {
            path: validated.path.clone(),
            source,
        })
    })?;
    let store = configuration_store(&app).map_err(WowDiscoveryCommandError::configuration)?;
    let mut configuration = store
        .load()
        .map_err(|error| WowDiscoveryCommandError::configuration(error.into()))?
        .configuration;

    select_validated_installation(&mut configuration, canonical_path, validated).map_err(
        |error| WowDiscoveryCommandError {
            code: WowDiscoveryCommandErrorCode::Configuration,
            message: error.to_string(),
        },
    )?;

    store
        .save(configuration)
        .map_err(|error| WowDiscoveryCommandError::configuration(error.into()))
}

fn select_validated_installation(
    configuration: &mut ManagerConfiguration,
    canonical_path: PathBuf,
    validated: ValidatedWowInstallation,
) -> Result<(), ConfigurationValidationError> {
    let existing_index = configuration.installations.iter().position(|installation| {
        fs::canonicalize(&installation.path)
            .map(|existing_path| existing_path == canonical_path)
            .unwrap_or(false)
    });
    let id = existing_index
        .map(|index| configuration.installations[index].id.clone())
        .unwrap_or_else(|| installation_id(&canonical_path, validated.product));
    let installation = WowInstallation {
        id: id.clone(),
        product: validated.product,
        path: canonical_path,
        availability: InstallationAvailability::Available,
    };
    if let Some(index) = existing_index {
        configuration.installations[index] = installation;
    } else {
        configuration.installations.push(installation);
    }
    configuration.select_installation(Some(id))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WowDiscoveryCommandError {
    pub code: WowDiscoveryCommandErrorCode,
    pub message: String,
}

impl WowDiscoveryCommandError {
    fn configuration(error: ConfigurationCommandError) -> Self {
        Self {
            code: WowDiscoveryCommandErrorCode::Configuration,
            message: error.message,
        }
    }

    fn invalid_path(error: WowValidationError) -> Self {
        Self {
            code: WowDiscoveryCommandErrorCode::InvalidInstallationPath,
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WowDiscoveryCommandErrorCode {
    Configuration,
    InvalidInstallationPath,
}

impl fmt::Display for WowDiscoveryCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WowDiscoveryCommandError {}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::configuration::ConfigurationStore;

    use super::*;

    #[test]
    fn selected_custom_installation_persists_through_manager_configuration() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before the Unix epoch")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("rpengine-manager-selected-wow-{unique}"));
        let installation_path = directory.join("custom-wow");
        fs::create_dir_all(installation_path.join("Data")).expect("create data directory");
        fs::write(installation_path.join("Wow.exe"), "test executable")
            .expect("create executable fixture");
        let canonical_path = fs::canonicalize(&installation_path).expect("canonicalize fixture");
        let validated = validate_installation_path(&canonical_path).expect("validate fixture");
        let store = ConfigurationStore::new(directory.join("configuration.json"));
        let mut configuration = ManagerConfiguration::default();

        select_validated_installation(&mut configuration, canonical_path.clone(), validated)
            .expect("select installation");
        let saved = store.save(configuration).expect("save configuration");
        let loaded = store.load().expect("load configuration");

        assert_eq!(loaded.configuration, saved);
        assert_eq!(loaded.configuration.installations[0].path, canonical_path);
        assert_eq!(
            loaded.configuration.selected_installation_id,
            Some(loaded.configuration.installations[0].id.clone())
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
