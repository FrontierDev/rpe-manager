//! Commands that expose Manager-owned configuration without exposing file paths.

use tauri::Manager;

use crate::configuration::{
    ConfigurationCommandError, ConfigurationLoad, ConfigurationStore, ManagerConfiguration,
};

const CONFIGURATION_FILE_NAME: &str = "configuration.json";

pub(crate) fn configuration_store(
    app: &tauri::AppHandle,
) -> Result<ConfigurationStore, ConfigurationCommandError> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| ConfigurationCommandError::configuration_path(error.to_string()))?;

    Ok(ConfigurationStore::new(
        directory.join(CONFIGURATION_FILE_NAME),
    ))
}

#[tauri::command]
pub fn load_manager_configuration(
    app: tauri::AppHandle,
) -> Result<ConfigurationLoad, ConfigurationCommandError> {
    configuration_store(&app)?.load().map_err(Into::into)
}

#[tauri::command]
pub fn save_manager_configuration(
    app: tauri::AppHandle,
    configuration: ManagerConfiguration,
) -> Result<ManagerConfiguration, ConfigurationCommandError> {
    configuration_store(&app)?
        .save(configuration)
        .map_err(Into::into)
}
