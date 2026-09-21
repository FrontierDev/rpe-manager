//! Read-only World of Warcraft installation discovery and validation.
//!
//! A folder name is only a product hint. Every candidate must contain the
//! expected game data directory and executable before it is returned as an
//! available installation.

use std::{
    collections::BTreeMap,
    env, fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::configuration::{InstallationAvailability, WowInstallation, WowProduct};

const WOW_DATA_DIRECTORY: &str = "Data";
const WOW_EXECUTABLE_NAMES: [&str; 2] = ["Wow.exe", "Wow-64.exe"];
const PRODUCT_DIRECTORIES: [(&str, WowProduct); 3] = [
    ("_retail_", WowProduct::Retail),
    ("_ptr_", WowProduct::Ptr),
    ("_beta_", WowProduct::Beta),
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WowInstallationCandidate {
    pub id: String,
    pub product: Option<WowProduct>,
    pub path: PathBuf,
    pub availability: InstallationAvailability,
    pub source: WowDiscoverySource,
    /// Present only for a previously configured path that can no longer be
    /// used. This lets the UI report that exact path rather than choosing a
    /// different installation automatically.
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WowDiscoverySource {
    Configured,
    CommonWindowsLocation,
    BattleNet,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedWowInstallation {
    pub product: Option<WowProduct>,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct DiscoveryInputs {
    pub configured_installations: Vec<WowInstallation>,
    pub common_installation_roots: Vec<PathBuf>,
    pub battle_net_product_databases: Vec<PathBuf>,
}

impl DiscoveryInputs {
    pub fn from_current_environment(configured_installations: Vec<WowInstallation>) -> Self {
        Self {
            configured_installations,
            common_installation_roots: common_installation_roots(),
            battle_net_product_databases: battle_net_product_databases(),
        }
    }
}

/// Builds discovery results without modifying any candidate directory.
pub fn discover(inputs: &DiscoveryInputs) -> Vec<WowInstallationCandidate> {
    let mut candidates = BTreeMap::new();

    for installation in &inputs.configured_installations {
        match validate_installation_path(&installation.path) {
            Ok(validated) => insert_candidate(
                &mut candidates,
                WowInstallationCandidate {
                    id: installation.id.clone(),
                    product: validated.product.or(installation.product),
                    path: validated.path,
                    availability: InstallationAvailability::Available,
                    source: WowDiscoverySource::Configured,
                    unavailable_reason: None,
                },
            ),
            Err(error) => insert_candidate(
                &mut candidates,
                WowInstallationCandidate {
                    id: installation.id.clone(),
                    product: installation.product,
                    path: installation.path.clone(),
                    availability: InstallationAvailability::Unavailable,
                    source: WowDiscoverySource::Configured,
                    unavailable_reason: Some(error.to_string()),
                },
            ),
        }
    }

    for root in &inputs.common_installation_roots {
        collect_product_children(
            &mut candidates,
            root,
            WowDiscoverySource::CommonWindowsLocation,
        );
    }

    for database_path in &inputs.battle_net_product_databases {
        for root in battle_net_installation_roots(database_path) {
            collect_root_and_product_children(
                &mut candidates,
                &root,
                WowDiscoverySource::BattleNet,
            );
        }
    }

    candidates.into_values().collect()
}

/// Verifies that a user-selected directory is an actual WoW installation.
pub fn validate_installation_path(
    path: &Path,
) -> Result<ValidatedWowInstallation, WowValidationError> {
    let metadata = fs::metadata(path).map_err(|source| WowValidationError::InspectPath {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_dir() {
        return Err(WowValidationError::NotDirectory(path.to_path_buf()));
    }

    let data_path = path.join(WOW_DATA_DIRECTORY);
    let data_metadata =
        fs::metadata(&data_path).map_err(|source| WowValidationError::MissingDataDirectory {
            path: data_path.clone(),
            source,
        })?;
    if !data_metadata.is_dir() {
        return Err(WowValidationError::DataPathIsNotDirectory(data_path));
    }

    let has_executable = WOW_EXECUTABLE_NAMES.iter().any(|name| {
        fs::metadata(path.join(name))
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    });
    if !has_executable {
        return Err(WowValidationError::MissingExecutable(path.to_path_buf()));
    }

    Ok(ValidatedWowInstallation {
        product: product_for_path(path),
        path: path.to_path_buf(),
    })
}

pub fn installation_id(path: &Path, product: Option<WowProduct>) -> String {
    let product_name = match product {
        Some(WowProduct::Retail) => "retail",
        Some(WowProduct::Ptr) => "ptr",
        Some(WowProduct::Beta) => "beta",
        None => "custom",
    };
    let hash = path
        .to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |value, byte| {
            (value ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });

    format!("wow-{product_name}-{hash:016x}")
}

fn insert_candidate(
    candidates: &mut BTreeMap<PathBuf, WowInstallationCandidate>,
    candidate: WowInstallationCandidate,
) {
    match candidates.get_mut(&candidate.path) {
        Some(existing) if existing.source == WowDiscoverySource::Configured => {}
        Some(existing) if candidate.source == WowDiscoverySource::Configured => {
            *existing = candidate
        }
        Some(_) => {}
        None => {
            candidates.insert(candidate.path.clone(), candidate);
        }
    }
}

fn collect_product_children(
    candidates: &mut BTreeMap<PathBuf, WowInstallationCandidate>,
    root: &Path,
    source: WowDiscoverySource,
) {
    for (directory_name, product) in PRODUCT_DIRECTORIES {
        let path = root.join(directory_name);
        if let Ok(validated) = validate_installation_path(&path) {
            insert_candidate(
                candidates,
                WowInstallationCandidate {
                    id: installation_id(&validated.path, Some(product)),
                    product: Some(product),
                    path: validated.path,
                    availability: InstallationAvailability::Available,
                    source,
                    unavailable_reason: None,
                },
            );
        }
    }
}

fn collect_root_and_product_children(
    candidates: &mut BTreeMap<PathBuf, WowInstallationCandidate>,
    root: &Path,
    source: WowDiscoverySource,
) {
    if let Ok(validated) = validate_installation_path(root) {
        insert_candidate(
            candidates,
            WowInstallationCandidate {
                id: installation_id(&validated.path, validated.product),
                product: validated.product,
                path: validated.path,
                availability: InstallationAvailability::Available,
                source,
                unavailable_reason: None,
            },
        );
    }
    collect_product_children(candidates, root, source);
}

fn product_for_path(path: &Path) -> Option<WowProduct> {
    let name = path.file_name()?.to_string_lossy();
    PRODUCT_DIRECTORIES
        .iter()
        .find(|(directory_name, _)| name.eq_ignore_ascii_case(directory_name))
        .map(|(_, product)| *product)
}

fn common_installation_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(directory) = env::var_os(variable) {
            roots.push(PathBuf::from(directory).join("World of Warcraft"));
        }
    }
    if let Some(system_drive) = env::var_os("SystemDrive") {
        let drive = system_drive.to_string_lossy();
        roots.push(PathBuf::from(format!(
            "{}\\World of Warcraft",
            drive.trim_end_matches(['\\', '/'])
        )));
    }
    roots.sort();
    roots.dedup();
    roots
}

fn battle_net_product_databases() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(program_data) = env::var_os("ProgramData") {
        paths.push(
            PathBuf::from(program_data)
                .join("Battle.net")
                .join("Agent")
                .join("product.db"),
        );
    }
    paths
}

fn battle_net_installation_roots(database_path: &Path) -> Vec<PathBuf> {
    let Ok(contents) = fs::read_to_string(database_path) else {
        return Vec::new();
    };
    let Ok(document) = serde_json::from_str::<Value>(&contents) else {
        return Vec::new();
    };

    let mut roots = Vec::new();
    collect_base_directories(&document, &mut roots);
    roots.sort();
    roots.dedup();
    roots
}

fn collect_base_directories(value: &Value, roots: &mut Vec<PathBuf>) {
    match value {
        Value::Object(object) => {
            if let Some(Value::String(path)) = object.get("baseDir") {
                roots.push(PathBuf::from(path));
            }
            for child in object.values() {
                collect_base_directories(child, roots);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_base_directories(child, roots);
            }
        }
        _ => {}
    }
}

#[derive(Debug)]
pub enum WowValidationError {
    DataPathIsNotDirectory(PathBuf),
    InspectPath { path: PathBuf, source: io::Error },
    MissingDataDirectory { path: PathBuf, source: io::Error },
    MissingExecutable(PathBuf),
    NotDirectory(PathBuf),
}

impl fmt::Display for WowValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataPathIsNotDirectory(path) => {
                write!(
                    formatter,
                    "WoW data path {} is not a directory",
                    path.display()
                )
            }
            Self::InspectPath { path, source } => {
                write!(formatter, "Could not inspect {}: {source}", path.display())
            }
            Self::MissingDataDirectory { path, source } => {
                write!(
                    formatter,
                    "Expected WoW data directory {}: {source}",
                    path.display()
                )
            }
            Self::MissingExecutable(path) => write!(
                formatter,
                "Expected Wow.exe or Wow-64.exe in {}",
                path.display()
            ),
            Self::NotDirectory(path) => write!(formatter, "{} is not a directory", path.display()),
        }
    }
}

impl std::error::Error for WowValidationError {}

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
            .expect("system clock is before Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rpengine-manager-discovery-{name}-{unique}"));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    fn wow_installation(root: &Path) {
        fs::create_dir_all(root.join(WOW_DATA_DIRECTORY)).expect("create data directory");
        fs::write(root.join("Wow.exe"), "test executable").expect("create executable fixture");
    }

    fn configured(path: PathBuf) -> WowInstallation {
        WowInstallation {
            id: "configured-retail".to_owned(),
            product: Some(WowProduct::Retail),
            path,
            availability: InstallationAvailability::Available,
        }
    }

    #[test]
    fn validates_a_standard_retail_structure() {
        let directory = test_directory("retail");
        let retail = directory.join("_retail_");
        wow_installation(&retail);

        let validated = validate_installation_path(&retail).expect("validate retail fixture");

        assert_eq!(validated.product, Some(WowProduct::Retail));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn validates_a_custom_structure_without_using_its_folder_name() {
        let directory = test_directory("custom");
        let custom = directory.join("my-game-copy");
        wow_installation(&custom);

        let validated = validate_installation_path(&custom).expect("validate custom fixture");

        assert_eq!(validated.product, None);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn rejects_an_arbitrary_folder_with_a_useful_reason() {
        let directory = test_directory("invalid");
        let arbitrary = directory.join("_retail_");
        fs::create_dir_all(&arbitrary).expect("create arbitrary directory");

        let error = validate_installation_path(&arbitrary).expect_err("reject arbitrary folder");

        assert!(error.to_string().contains("Expected WoW data directory"));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn retains_a_missing_configured_path_as_unavailable() {
        let directory = test_directory("missing-configured");
        let missing = directory.join("_retail_");
        let results = discover(&DiscoveryInputs {
            configured_installations: vec![configured(missing)],
            common_installation_roots: Vec::new(),
            battle_net_product_databases: Vec::new(),
        });

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].availability,
            InstallationAvailability::Unavailable
        );
        assert_eq!(results[0].source, WowDiscoverySource::Configured);
        assert!(results[0].unavailable_reason.is_some());
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn returns_multiple_distinct_installations_from_common_and_battle_net_locations() {
        let directory = test_directory("multiple");
        let common_root = directory.join("common").join("World of Warcraft");
        let retail = common_root.join("_retail_");
        let battle_net_root = directory.join("battle-net").join("World of Warcraft");
        let ptr = battle_net_root.join("_ptr_");
        wow_installation(&retail);
        wow_installation(&ptr);
        let product_database = directory.join("product.db");
        fs::write(
            &product_database,
            serde_json::json!({ "products": [{ "baseDir": battle_net_root }] }).to_string(),
        )
        .expect("write Battle.net fixture");

        let results = discover(&DiscoveryInputs {
            configured_installations: Vec::new(),
            common_installation_roots: vec![common_root],
            battle_net_product_databases: vec![product_database],
        });

        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .any(|candidate| candidate.product == Some(WowProduct::Retail)));
        assert!(results.iter().any(|candidate| {
            candidate.product == Some(WowProduct::Ptr)
                && candidate.source == WowDiscoverySource::BattleNet
        }));
        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
