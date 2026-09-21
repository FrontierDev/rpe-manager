//! Versioned, Manager-owned local configuration.
//!
//! This module only stores Manager choices and never reads or writes WoW or RPE
//! data files. The configuration file is resolved by the Tauri backend, so the
//! frontend cannot select an arbitrary filesystem location for it.

use std::{collections::BTreeMap, fmt, fs, io, path::PathBuf};

use serde::{Deserialize, Serialize};

pub const CONFIGURATION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerConfiguration {
    #[serde(default = "current_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub installations: Vec<WowInstallation>,
    #[serde(default)]
    pub selected_installation_id: Option<String>,
    #[serde(default)]
    pub selected_account_ids: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub preferences: ManagerPreferences,
    /// Reserved for a later catalogue integration. No catalogue behaviour is
    /// implemented by this configuration module.
    #[serde(default)]
    pub catalogue_url: Option<String>,
}

impl Default for ManagerConfiguration {
    fn default() -> Self {
        Self {
            schema_version: CONFIGURATION_SCHEMA_VERSION,
            installations: Vec::new(),
            selected_installation_id: None,
            selected_account_ids: BTreeMap::new(),
            preferences: ManagerPreferences::default(),
            catalogue_url: None,
        }
    }
}

impl ManagerConfiguration {
    pub fn select_installation(
        &mut self,
        installation_id: Option<String>,
    ) -> Result<(), ConfigurationValidationError> {
        if let Some(installation_id) = installation_id.as_deref() {
            self.require_installation(installation_id)?;
        }

        self.selected_installation_id = installation_id;
        Ok(())
    }

    pub fn set_selected_accounts(
        &mut self,
        installation_id: &str,
        account_ids: Vec<String>,
    ) -> Result<(), ConfigurationValidationError> {
        self.require_installation(installation_id)?;

        if account_ids
            .iter()
            .any(|account_id| account_id.trim().is_empty())
        {
            return Err(ConfigurationValidationError::EmptyAccountIdentifier);
        }

        let mut account_ids = account_ids;
        account_ids.sort();
        account_ids.dedup();

        // Keep an empty vector: its presence records an explicit “select no
        // accounts” choice, distinct from an installation with no preference.
        self.selected_account_ids
            .insert(installation_id.to_owned(), account_ids);

        Ok(())
    }

    fn prepare_for_storage(&mut self) -> Result<(), ConfigurationValidationError> {
        self.installations
            .sort_by(|left, right| left.id.cmp(&right.id));

        for account_ids in self.selected_account_ids.values_mut() {
            account_ids.sort();
            account_ids.dedup();
        }

        self.validate()
    }

    fn refresh_path_availability(&mut self) -> Result<(), ConfigurationError> {
        for installation in &mut self.installations {
            installation.availability = match installation.path.try_exists() {
                Ok(true) => InstallationAvailability::Available,
                Ok(false) => InstallationAvailability::Unavailable,
                Err(error) => {
                    return Err(ConfigurationError::PathInspection {
                        path: installation.path.clone(),
                        source: error,
                    });
                }
            };
        }

        Ok(())
    }

    fn validate(&self) -> Result<(), ConfigurationValidationError> {
        if self.schema_version != CONFIGURATION_SCHEMA_VERSION {
            return Err(ConfigurationValidationError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }

        let mut previous_id: Option<&str> = None;
        for installation in &self.installations {
            if installation.id.trim().is_empty() {
                return Err(ConfigurationValidationError::EmptyInstallationIdentifier);
            }

            if installation.path.as_os_str().is_empty() {
                return Err(ConfigurationValidationError::EmptyInstallationPath(
                    installation.id.clone(),
                ));
            }

            if previous_id == Some(installation.id.as_str()) {
                return Err(
                    ConfigurationValidationError::DuplicateInstallationIdentifier(
                        installation.id.clone(),
                    ),
                );
            }
            previous_id = Some(&installation.id);
        }

        if let Some(installation_id) = self.selected_installation_id.as_deref() {
            self.require_installation(installation_id)?;
        }

        for (installation_id, account_ids) in &self.selected_account_ids {
            self.require_installation(installation_id)?;
            if account_ids
                .iter()
                .any(|account_id| account_id.trim().is_empty())
            {
                return Err(ConfigurationValidationError::EmptyAccountIdentifier);
            }
        }

        Ok(())
    }

    fn require_installation(
        &self,
        installation_id: &str,
    ) -> Result<(), ConfigurationValidationError> {
        if self
            .installations
            .iter()
            .any(|installation| installation.id == installation_id)
        {
            Ok(())
        } else {
            Err(ConfigurationValidationError::UnknownInstallationIdentifier(
                installation_id.to_owned(),
            ))
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WowInstallation {
    pub id: String,
    /// The product is absent for installations that do not match a known WoW
    /// product root.
    pub product: Option<WowProduct>,
    pub path: PathBuf,
    #[serde(default)]
    pub availability: InstallationAvailability,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WowProduct {
    Retail,
    Ptr,
    Beta,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationAvailability {
    Available,
    #[default]
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerPreferences {
    pub backup_retention: u16,
    pub update_checking_enabled: bool,
}

impl Default for ManagerPreferences {
    fn default() -> Self {
        Self {
            backup_retention: 10,
            update_checking_enabled: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationLoad {
    pub configuration: ManagerConfiguration,
    pub recovery: Option<ConfigurationRecovery>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationRecovery {
    pub code: ConfigurationRecoveryCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationRecoveryCode {
    InvalidConfiguration,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationCommandError {
    pub code: ConfigurationCommandErrorCode,
    pub message: String,
}

impl ConfigurationCommandError {
    pub fn configuration_path(message: String) -> Self {
        Self {
            code: ConfigurationCommandErrorCode::ConfigurationPath,
            message,
        }
    }
}

impl From<ConfigurationError> for ConfigurationCommandError {
    fn from(error: ConfigurationError) -> Self {
        let code = match &error {
            ConfigurationError::Read { .. } => ConfigurationCommandErrorCode::Read,
            ConfigurationError::Validation(_) => ConfigurationCommandErrorCode::Validation,
            ConfigurationError::Write { .. } => ConfigurationCommandErrorCode::Write,
            ConfigurationError::PathInspection { .. } => {
                ConfigurationCommandErrorCode::PathInspection
            }
        };

        Self {
            code,
            message: error.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationCommandErrorCode {
    ConfigurationPath,
    PathInspection,
    Read,
    Validation,
    Write,
}

#[derive(Debug)]
pub struct ConfigurationStore {
    path: PathBuf,
}

impl ConfigurationStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<ConfigurationLoad, ConfigurationError> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(ConfigurationLoad {
                    configuration: ManagerConfiguration::default(),
                    recovery: None,
                });
            }
            Err(error) => {
                return Err(ConfigurationError::Read {
                    path: self.path.clone(),
                    source: error,
                });
            }
        };

        let mut configuration = match serde_json::from_str::<ManagerConfiguration>(&contents) {
            Ok(configuration) => configuration,
            Err(error) => return Ok(Self::invalid_configuration_recovery(error.to_string())),
        };

        if let Err(error) = configuration.prepare_for_storage() {
            return Ok(Self::invalid_configuration_recovery(error.to_string()));
        }

        configuration.refresh_path_availability()?;

        Ok(ConfigurationLoad {
            configuration,
            recovery: None,
        })
    }

    pub fn save(
        &self,
        mut configuration: ManagerConfiguration,
    ) -> Result<ManagerConfiguration, ConfigurationError> {
        configuration
            .prepare_for_storage()
            .map_err(ConfigurationError::Validation)?;
        configuration.refresh_path_availability()?;

        let parent = self
            .path
            .parent()
            .ok_or_else(|| ConfigurationError::Write {
                path: self.path.clone(),
                source: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "configuration file has no parent directory",
                ),
            })?;
        fs::create_dir_all(parent).map_err(|source| ConfigurationError::Write {
            path: parent.to_path_buf(),
            source,
        })?;

        let contents = serde_json::to_string_pretty(&configuration).map_err(|error| {
            ConfigurationError::Write {
                path: self.path.clone(),
                source: io::Error::new(io::ErrorKind::InvalidData, error),
            }
        })?;
        fs::write(&self.path, format!("{contents}\n")).map_err(|source| {
            ConfigurationError::Write {
                path: self.path.clone(),
                source,
            }
        })?;

        Ok(configuration)
    }

    fn invalid_configuration_recovery(message: String) -> ConfigurationLoad {
        ConfigurationLoad {
            configuration: ManagerConfiguration::default(),
            recovery: Some(ConfigurationRecovery {
                code: ConfigurationRecoveryCode::InvalidConfiguration,
                message,
            }),
        }
    }
}

#[derive(Debug)]
pub enum ConfigurationError {
    PathInspection { path: PathBuf, source: io::Error },
    Read { path: PathBuf, source: io::Error },
    Validation(ConfigurationValidationError),
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathInspection { path, source } => {
                write!(
                    formatter,
                    "Could not inspect configured path {}: {source}",
                    path.display()
                )
            }
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "Could not read configuration {}: {source}",
                    path.display()
                )
            }
            Self::Validation(error) => write!(formatter, "Invalid configuration: {error}"),
            Self::Write { path, source } => {
                write!(
                    formatter,
                    "Could not write configuration {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ConfigurationError {}

#[derive(Debug)]
pub enum ConfigurationValidationError {
    DuplicateInstallationIdentifier(String),
    EmptyAccountIdentifier,
    EmptyInstallationIdentifier,
    EmptyInstallationPath(String),
    UnknownInstallationIdentifier(String),
    UnsupportedSchemaVersion(u32),
}

impl fmt::Display for ConfigurationValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateInstallationIdentifier(identifier) => {
                write!(formatter, "duplicate installation identifier: {identifier}")
            }
            Self::EmptyAccountIdentifier => write!(formatter, "an account identifier is empty"),
            Self::EmptyInstallationIdentifier => {
                write!(formatter, "an installation identifier is empty")
            }
            Self::EmptyInstallationPath(identifier) => {
                write!(formatter, "installation {identifier} has an empty path")
            }
            Self::UnknownInstallationIdentifier(identifier) => {
                write!(formatter, "unknown installation identifier: {identifier}")
            }
            Self::UnsupportedSchemaVersion(version) => {
                write!(formatter, "unsupported schema version: {version}")
            }
        }
    }
}

impl std::error::Error for ConfigurationValidationError {}

fn current_schema_version() -> u32 {
    CONFIGURATION_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rpengine-manager-{name}-{unique}"));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    fn installation(id: &str, path: PathBuf) -> WowInstallation {
        WowInstallation {
            id: id.to_owned(),
            product: Some(WowProduct::Retail),
            path,
            availability: InstallationAvailability::Unavailable,
        }
    }

    #[test]
    fn default_configuration_serializes_and_deserializes_deterministically() {
        let serialized = serde_json::to_string_pretty(&ManagerConfiguration::default())
            .expect("serialize default configuration");
        let deserialized: ManagerConfiguration =
            serde_json::from_str(&serialized).expect("deserialize default configuration");

        assert_eq!(deserialized, ManagerConfiguration::default());
        assert_eq!(
            serialized,
            "{\n  \"schemaVersion\": 1,\n  \"installations\": [],\n  \"selectedInstallationId\": null,\n  \"selectedAccountIds\": {},\n  \"preferences\": {\n    \"backupRetention\": 10,\n    \"updateCheckingEnabled\": true\n  },\n  \"catalogueUrl\": null\n}"
        );
    }

    #[test]
    fn selections_are_explicit_and_account_identifiers_are_canonicalized() {
        let path = test_directory("state-transitions");
        let mut configuration = ManagerConfiguration {
            installations: vec![
                installation("retail", path.clone()),
                installation("ptr", path.clone()),
            ],
            ..ManagerConfiguration::default()
        };

        configuration
            .select_installation(Some("ptr".to_owned()))
            .expect("select configured installation");
        configuration
            .set_selected_accounts(
                "ptr",
                vec![
                    "ACCOUNT_B".to_owned(),
                    "ACCOUNT_A".to_owned(),
                    "ACCOUNT_B".to_owned(),
                ],
            )
            .expect("select configured accounts");

        assert_eq!(
            configuration.selected_installation_id.as_deref(),
            Some("ptr")
        );
        assert_eq!(
            configuration.selected_account_ids.get("ptr"),
            Some(&vec!["ACCOUNT_A".to_owned(), "ACCOUNT_B".to_owned()])
        );
        assert!(configuration
            .select_installation(Some("missing".to_owned()))
            .is_err());
        assert_eq!(
            configuration.selected_installation_id.as_deref(),
            Some("ptr")
        );

        fs::remove_dir_all(path).expect("remove test directory");
    }

    #[test]
    fn configuration_persists_multiple_installations_and_selected_accounts() {
        let directory = test_directory("persistence");
        let store = ConfigurationStore::new(directory.join("configuration.json"));
        let mut configuration = ManagerConfiguration {
            installations: vec![
                installation("retail", directory.clone()),
                WowInstallation {
                    id: "custom".to_owned(),
                    product: None,
                    path: directory.clone(),
                    availability: InstallationAvailability::Unavailable,
                },
            ],
            ..ManagerConfiguration::default()
        };
        configuration
            .select_installation(Some("custom".to_owned()))
            .expect("select custom installation");
        configuration
            .set_selected_accounts("custom", vec!["ACCOUNT_ONE".to_owned()])
            .expect("select account");

        let saved = store.save(configuration).expect("save configuration");
        let loaded = store.load().expect("load configuration");

        assert_eq!(loaded.recovery, None);
        assert_eq!(loaded.configuration, saved);
        assert_eq!(loaded.configuration.installations.len(), 2);
        assert_eq!(
            loaded.configuration.selected_installation_id.as_deref(),
            Some("custom")
        );
        assert_eq!(
            loaded.configuration.selected_account_ids.get("custom"),
            Some(&vec!["ACCOUNT_ONE".to_owned()])
        );

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn configuration_persists_an_explicit_empty_account_selection() {
        let directory = test_directory("empty-account-selection");
        let store = ConfigurationStore::new(directory.join("configuration.json"));
        let mut configuration = ManagerConfiguration {
            installations: vec![installation("retail", directory.clone())],
            ..ManagerConfiguration::default()
        };
        configuration
            .set_selected_accounts("retail", Vec::new())
            .expect("save explicit empty selection");

        let saved = store.save(configuration).expect("save configuration");
        let loaded = store.load().expect("load configuration");

        assert_eq!(saved.selected_account_ids.get("retail"), Some(&Vec::new()));
        assert_eq!(
            loaded.configuration.selected_account_ids.get("retail"),
            Some(&Vec::new())
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn missing_configured_path_is_retained_as_unavailable() {
        let directory = test_directory("missing-path");
        let missing_path = directory.join("missing-installation");
        let store = ConfigurationStore::new(directory.join("configuration.json"));
        let mut configuration = ManagerConfiguration {
            installations: vec![installation("retail", missing_path)],
            ..ManagerConfiguration::default()
        };
        configuration
            .select_installation(Some("retail".to_owned()))
            .expect("select missing installation");

        store.save(configuration).expect("save configuration");
        let loaded = store.load().expect("load configuration");

        assert_eq!(
            loaded.configuration.selected_installation_id.as_deref(),
            Some("retail")
        );
        assert_eq!(
            loaded.configuration.installations[0].availability,
            InstallationAvailability::Unavailable
        );

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn malformed_configuration_returns_explicit_recovery_state() {
        let directory = test_directory("malformed");
        let store = ConfigurationStore::new(directory.join("configuration.json"));
        fs::write(store.path.as_path(), "{not valid JSON").expect("write malformed configuration");

        let loaded = store.load().expect("load malformed configuration");

        assert_eq!(loaded.configuration, ManagerConfiguration::default());
        assert_eq!(
            loaded.recovery.map(|recovery| recovery.code),
            Some(ConfigurationRecoveryCode::InvalidConfiguration)
        );

        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
