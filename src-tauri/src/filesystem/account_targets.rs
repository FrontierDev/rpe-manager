//! Resolves selected account SavedVariables paths from validated configuration.

use std::{
    fmt,
    path::{Component, Path, PathBuf},
};

use crate::{
    configuration::{InstallationAvailability, ManagerConfiguration, WowInstallation},
    discovery::{accounts::is_structural_account_directory, wow::validate_installation_path},
};

pub(crate) const SAVED_VARIABLES_FILE_NAME: &str = "RPEngine2.lua";

#[derive(Clone, Debug)]
pub(crate) struct SavedVariablesAccountTarget {
    pub installation_id: String,
    pub account_id: String,
    pub saved_variables_path: PathBuf,
}

pub(crate) fn resolve_selected_saved_variables_targets(
    configuration: &ManagerConfiguration,
) -> Result<Vec<SavedVariablesAccountTarget>, SavedVariablesTargetError> {
    let selected_id = configuration
        .selected_installation_id
        .as_deref()
        .ok_or(SavedVariablesTargetError::NoSelectedInstallation)?;
    let installation = configuration
        .installations
        .iter()
        .find(|candidate| candidate.id == selected_id)
        .ok_or_else(|| {
            SavedVariablesTargetError::SelectedInstallationMissing(selected_id.to_owned())
        })?;
    validate_selected_installation(installation)?;

    let account_ids = configuration
        .selected_account_ids
        .get(selected_id)
        .filter(|accounts| !accounts.is_empty())
        .ok_or_else(|| SavedVariablesTargetError::NoSelectedAccounts(selected_id.to_owned()))?;

    account_ids
        .iter()
        .map(|account_id| {
            validate_account_identifier(account_id)?;
            Ok(SavedVariablesAccountTarget {
                installation_id: installation.id.clone(),
                account_id: account_id.clone(),
                saved_variables_path: installation
                    .path
                    .join("WTF")
                    .join("Account")
                    .join(account_id)
                    .join("SavedVariables")
                    .join(SAVED_VARIABLES_FILE_NAME),
            })
        })
        .collect()
}

fn validate_selected_installation(
    installation: &WowInstallation,
) -> Result<(), SavedVariablesTargetError> {
    if installation.availability != InstallationAvailability::Available {
        return Err(SavedVariablesTargetError::InstallationUnavailable(
            installation.id.clone(),
        ));
    }
    validate_installation_path(&installation.path).map_err(|error| {
        SavedVariablesTargetError::InstallationValidation {
            installation_id: installation.id.clone(),
            message: error.to_string(),
        }
    })?;
    Ok(())
}

fn validate_account_identifier(account_id: &str) -> Result<(), SavedVariablesTargetError> {
    if is_structural_account_directory(account_id) {
        return Err(SavedVariablesTargetError::InvalidAccountIdentifier(
            account_id.to_owned(),
        ));
    }
    let mut components = Path::new(account_id).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) if !account_id.trim().is_empty() => Ok(()),
        _ => Err(SavedVariablesTargetError::InvalidAccountIdentifier(
            account_id.to_owned(),
        )),
    }
}

#[derive(Debug)]
pub enum SavedVariablesTargetError {
    InstallationUnavailable(String),
    InstallationValidation {
        installation_id: String,
        message: String,
    },
    InvalidAccountIdentifier(String),
    NoSelectedAccounts(String),
    NoSelectedInstallation,
    SelectedInstallationMissing(String),
}

impl fmt::Display for SavedVariablesTargetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InstallationUnavailable(installation_id) => write!(
                formatter,
                "Selected installation {installation_id} is unavailable."
            ),
            Self::InstallationValidation {
                installation_id,
                message,
            } => write!(
                formatter,
                "Selected installation {installation_id} is no longer valid: {message}"
            ),
            Self::InvalidAccountIdentifier(account_id) => {
                write!(
                    formatter,
                    "Selected account identifier {account_id:?} is invalid"
                )
            }
            Self::NoSelectedAccounts(installation_id) => write!(
                formatter,
                "No accounts are selected for installation {installation_id}."
            ),
            Self::NoSelectedInstallation => {
                formatter.write_str("No World of Warcraft installation is selected.")
            }
            Self::SelectedInstallationMissing(installation_id) => write!(
                formatter,
                "Selected installation {installation_id} no longer exists in configuration."
            ),
        }
    }
}

impl std::error::Error for SavedVariablesTargetError {}
