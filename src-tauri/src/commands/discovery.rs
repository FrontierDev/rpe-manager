//! Tauri commands for read-only WoW discovery and explicit installation choice.

use std::{fmt, fs, path::PathBuf};

use serde::Serialize;

use crate::{
    commands::configuration::configuration_store,
    configuration::{
        ConfigurationCommandError, ConfigurationValidationError, InstallationAvailability,
        ManagerConfiguration, WowInstallation,
    },
    discovery::{
        accounts::discover_accounts,
        wow::{
            default_installation_candidate, discover, installation_id,
            resolve_installation_selection, DiscoveryInputs, InstallationSelection,
            ValidatedWowInstallation, WowInstallationCandidate, WowValidationError,
        },
    },
};

#[tauri::command]
pub fn discover_wow_installations(
    app: tauri::AppHandle,
) -> Result<Vec<WowInstallationCandidate>, WowDiscoveryCommandError> {
    let store = configuration_store(&app).map_err(WowDiscoveryCommandError::configuration)?;
    let mut configuration = store
        .load()
        .map_err(|error| WowDiscoveryCommandError::configuration(error.into()))?
        .configuration;
    let candidates = discover(&DiscoveryInputs::from_current_environment(
        configuration.installations.clone(),
    ));
    ensure_default_installation(&mut configuration, &candidates)?;
    store
        .save(configuration)
        .map_err(|error| WowDiscoveryCommandError::configuration(error.into()))?;
    Ok(candidates)
}

/// Validates a folder chosen through the native dialog, then stores and selects
/// only that exact installation. It never changes files under the WoW folder.
#[tauri::command]
pub fn select_wow_installation(
    app: tauri::AppHandle,
    path: PathBuf,
) -> Result<WowInstallationSelectionResult, WowDiscoveryCommandError> {
    let selection =
        resolve_installation_selection(&path).map_err(WowDiscoveryCommandError::invalid_path)?;
    let validated = match selection {
        InstallationSelection::Exact(validated) => validated,
        InstallationSelection::MultipleProducts(products) => {
            return Ok(WowInstallationSelectionResult::MultipleProducts {
                products: products
                    .into_iter()
                    .map(|product| WowInstallationProductChoice {
                        product: product
                            .product
                            .expect("supported product child has a product"),
                        path: product.path,
                    })
                    .collect(),
            });
        }
    };
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
    select_discovered_accounts_if_unset(&mut configuration).map_err(|error| {
        WowDiscoveryCommandError {
            code: WowDiscoveryCommandErrorCode::Configuration,
            message: error,
        }
    })?;

    store
        .save(configuration)
        .map(|configuration| WowInstallationSelectionResult::Configured { configuration })
        .map_err(|error| WowDiscoveryCommandError::configuration(error.into()))
}

/// First configuration of an installation selects every real account. Once an
/// installation has a map entry (including an empty one), that persisted user
/// choice is authoritative; newly discovered accounts are not auto-selected.
fn select_discovered_accounts_if_unset(
    configuration: &mut ManagerConfiguration,
) -> Result<(), String> {
    let installation_id = configuration
        .selected_installation_id
        .clone()
        .ok_or_else(|| "No installation was selected.".to_owned())?;
    if configuration
        .selected_account_ids
        .contains_key(&installation_id)
    {
        return Ok(());
    }
    let installation = configuration
        .installations
        .iter()
        .find(|installation| installation.id == installation_id)
        .ok_or_else(|| "Selected installation no longer exists in configuration.".to_owned())?;
    let accounts = discover_accounts(&installation.path)
        .map_err(|error| format!("Could not discover accounts: {error}"))?;
    configuration
        .set_selected_accounts(
            &installation_id,
            accounts.into_iter().map(|account| account.id).collect(),
        )
        .map_err(|error| error.to_string())
}

fn ensure_default_installation(
    configuration: &mut ManagerConfiguration,
    candidates: &[WowInstallationCandidate],
) -> Result<(), WowDiscoveryCommandError> {
    let has_valid_selection = configuration
        .selected_installation_id
        .as_deref()
        .is_some_and(|id| {
            candidates.iter().any(|candidate| {
                candidate.id == id && candidate.availability == InstallationAvailability::Available
            })
        });
    if has_valid_selection {
        return Ok(());
    }
    let Some(candidate) = default_installation_candidate(candidates) else {
        return Ok(());
    };
    let installation = WowInstallation {
        id: candidate.id.clone(),
        product: candidate.product,
        path: candidate.path.clone(),
        availability: InstallationAvailability::Available,
    };
    if let Some(existing) = configuration
        .installations
        .iter_mut()
        .find(|existing| existing.id == candidate.id)
    {
        *existing = installation;
    } else {
        configuration.installations.push(installation);
    }
    configuration
        .select_installation(Some(candidate.id.clone()))
        .map_err(|error| WowDiscoveryCommandError {
            code: WowDiscoveryCommandErrorCode::Configuration,
            message: error.to_string(),
        })?;
    select_discovered_accounts_if_unset(configuration).map_err(|message| WowDiscoveryCommandError {
        code: WowDiscoveryCommandErrorCode::Configuration,
        message,
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WowInstallationSelectionResult {
    Configured {
        configuration: ManagerConfiguration,
    },
    MultipleProducts {
        products: Vec<WowInstallationProductChoice>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WowInstallationProductChoice {
    pub product: crate::configuration::WowProduct,
    pub path: PathBuf,
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

    fn candidate(product: crate::configuration::WowProduct, id: &str) -> WowInstallationCandidate {
        WowInstallationCandidate {
            id: id.to_owned(),
            product: Some(product),
            path: PathBuf::from(format!("C:/Games/{id}")),
            availability: InstallationAvailability::Available,
            source: crate::discovery::wow::WowDiscoverySource::BattleNet,
            unavailable_reason: None,
        }
    }

    #[test]
    fn preserves_a_valid_explicit_ptr_selection_over_the_retail_default() {
        let ptr = candidate(crate::configuration::WowProduct::Ptr, "ptr");
        let retail = candidate(crate::configuration::WowProduct::Retail, "retail");
        let mut configuration = ManagerConfiguration {
            installations: vec![WowInstallation {
                id: "ptr".to_owned(),
                product: ptr.product,
                path: ptr.path.clone(),
                availability: InstallationAvailability::Available,
            }],
            selected_installation_id: Some("ptr".to_owned()),
            ..ManagerConfiguration::default()
        };

        ensure_default_installation(&mut configuration, &[retail, ptr])
            .expect("preserve valid explicit selection");

        assert_eq!(
            configuration.selected_installation_id.as_deref(),
            Some("ptr")
        );
    }

    #[test]
    fn replaces_an_invalid_selection_with_retail() {
        let retail = candidate(crate::configuration::WowProduct::Retail, "retail");
        let mut configuration = ManagerConfiguration {
            selected_installation_id: Some("missing".to_owned()),
            ..ManagerConfiguration::default()
        };

        ensure_default_installation(&mut configuration, &[retail]).expect("select retail fallback");

        assert_eq!(
            configuration.selected_installation_id.as_deref(),
            Some("retail")
        );
    }

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
        fs::create_dir_all(installation_path.join("WTF/Account/ACCOUNT_A"))
            .expect("create account A");
        fs::create_dir_all(installation_path.join("WTF/Account/ACCOUNT_B"))
            .expect("create account B");
        fs::create_dir_all(installation_path.join("WTF/Account/SavedVariables"))
            .expect("create structural directory");
        fs::write(installation_path.join("Wow.exe"), "test executable")
            .expect("create executable fixture");
        let canonical_path = fs::canonicalize(&installation_path).expect("canonicalize fixture");
        let validated = match resolve_installation_selection(&canonical_path)
            .expect("validate fixture")
        {
            InstallationSelection::Exact(validated) => validated,
            InstallationSelection::MultipleProducts(_) => panic!("custom fixture should be exact"),
        };
        let store = ConfigurationStore::new(directory.join("configuration.json"));
        let mut configuration = ManagerConfiguration::default();

        select_validated_installation(&mut configuration, canonical_path.clone(), validated)
            .expect("select installation");
        select_discovered_accounts_if_unset(&mut configuration)
            .expect("select discovered accounts");
        let saved = store.save(configuration).expect("save configuration");
        let loaded = store.load().expect("load configuration");

        assert_eq!(loaded.configuration, saved);
        assert_eq!(loaded.configuration.installations[0].path, canonical_path);
        assert_eq!(
            loaded.configuration.selected_installation_id,
            Some(loaded.configuration.installations[0].id.clone())
        );
        assert_eq!(
            loaded
                .configuration
                .selected_account_ids
                .get(&loaded.configuration.installations[0].id),
            Some(&vec!["ACCOUNT_A".to_owned(), "ACCOUNT_B".to_owned()])
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn explicit_account_selection_is_not_replaced_when_accounts_change() {
        let directory = std::env::temp_dir().join("rpengine-manager-account-choice");
        let installation_path = directory.join("custom-wow");
        fs::create_dir_all(installation_path.join("Data")).expect("create data directory");
        fs::create_dir_all(installation_path.join("WTF/Account/ACCOUNT_A"))
            .expect("create account A");
        fs::create_dir_all(installation_path.join("WTF/Account/ACCOUNT_B"))
            .expect("create account B");
        fs::write(installation_path.join("Wow.exe"), "test executable")
            .expect("create executable fixture");
        let canonical_path = fs::canonicalize(&installation_path).expect("canonicalize fixture");
        let validated = match resolve_installation_selection(&canonical_path)
            .expect("validate fixture")
        {
            InstallationSelection::Exact(validated) => validated,
            InstallationSelection::MultipleProducts(_) => panic!("custom fixture should be exact"),
        };
        let mut configuration = ManagerConfiguration::default();
        select_validated_installation(&mut configuration, canonical_path, validated)
            .expect("select installation");
        let installation_id = configuration
            .selected_installation_id
            .clone()
            .expect("selected id");
        configuration
            .set_selected_accounts(&installation_id, vec!["ACCOUNT_A".to_owned()])
            .expect("save explicit selection");

        select_discovered_accounts_if_unset(&mut configuration)
            .expect("preserve explicit selection");

        assert_eq!(
            configuration.selected_account_ids.get(&installation_id),
            Some(&vec!["ACCOUNT_A".to_owned()])
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn root_selection_persists_the_resolved_product_directory() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before the Unix epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("rpengine-manager-root-wow-{unique}"));
        let root = directory.join("World of Warcraft");
        let retail = root.join("_retail_");
        fs::create_dir_all(retail.join("Data")).expect("create data directory");
        fs::write(retail.join("Wow.exe"), "test executable").expect("create executable");
        let validated = match resolve_installation_selection(&root).expect("resolve root") {
            InstallationSelection::Exact(validated) => validated,
            InstallationSelection::MultipleProducts(_) => {
                panic!("one product should resolve exactly")
            }
        };
        let canonical_retail = fs::canonicalize(&retail).expect("canonicalize retail");
        let store = ConfigurationStore::new(directory.join("configuration.json"));
        let mut configuration = ManagerConfiguration::default();

        select_validated_installation(&mut configuration, canonical_retail.clone(), validated)
            .expect("select installation");
        store.save(configuration).expect("save configuration");
        let loaded = store.load().expect("load configuration");

        assert_eq!(loaded.configuration.installations[0].path, canonical_retail);
        assert_eq!(
            loaded.configuration.selected_installation_id,
            Some(loaded.configuration.installations[0].id.clone())
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
