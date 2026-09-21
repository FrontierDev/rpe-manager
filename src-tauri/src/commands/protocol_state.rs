//! Read-only Tauri commands for persisted RPE external-management state.

use crate::{
    commands::configuration::configuration_store,
    configuration::ConfigurationCommandError,
    filesystem::protocol_state::{
        read_selected_protocol_state, reconcile_request, ProtocolStateReadError,
        RequestReconciliationReport, SelectedProtocolState,
    },
    processes::wow::inspect_wow_processes,
};
use serde::{Deserialize, Serialize};

#[tauri::command]
pub fn get_selected_protocol_state(
    app: tauri::AppHandle,
) -> Result<SelectedProtocolState, ProtocolStateCommandError> {
    let configuration = load_configuration(&app)?;
    read_selected_protocol_state(&configuration, &inspect_wow_processes()).map_err(Into::into)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileProtocolRequest {
    pub request_id: String,
}

#[tauri::command]
pub fn reconcile_selected_protocol_request(
    app: tauri::AppHandle,
    request: ReconcileProtocolRequest,
) -> Result<RequestReconciliationReport, ProtocolStateCommandError> {
    let configuration = load_configuration(&app)?;
    let snapshot = read_selected_protocol_state(&configuration, &inspect_wow_processes())
        .map_err(ProtocolStateCommandError::from)?;
    reconcile_request(&snapshot, &request.request_id).map_err(Into::into)
}

fn load_configuration(
    app: &tauri::AppHandle,
) -> Result<crate::configuration::ManagerConfiguration, ProtocolStateCommandError> {
    configuration_store(app)
        .map_err(ProtocolStateCommandError::configuration)?
        .load()
        .map_err(|error| ProtocolStateCommandError::configuration(error.into()))
        .map(|load| load.configuration)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolStateCommandError {
    pub code: ProtocolStateCommandErrorCode,
    pub message: String,
}

impl ProtocolStateCommandError {
    fn configuration(error: ConfigurationCommandError) -> Self {
        Self {
            code: ProtocolStateCommandErrorCode::Configuration,
            message: error.message,
        }
    }
}

impl From<ProtocolStateReadError> for ProtocolStateCommandError {
    fn from(error: ProtocolStateReadError) -> Self {
        Self {
            code: ProtocolStateCommandErrorCode::ProtocolState,
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolStateCommandErrorCode {
    Configuration,
    ProtocolState,
}
