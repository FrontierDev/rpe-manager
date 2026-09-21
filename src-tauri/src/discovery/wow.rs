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
    pub battle_net_configuration_files: Vec<PathBuf>,
    pub registry_installation_roots: Vec<PathBuf>,
}

/// Returns the preferred available installation. This is the sole product
/// priority policy: Retail, PTR, Beta, then a custom installation.
pub fn default_installation_candidate(
    candidates: &[WowInstallationCandidate],
) -> Option<&WowInstallationCandidate> {
    candidates
        .iter()
        .filter(|candidate| candidate.availability == InstallationAvailability::Available)
        .min_by_key(|candidate| (product_priority(candidate.product), candidate.path.clone()))
}

impl DiscoveryInputs {
    pub fn from_current_environment(configured_installations: Vec<WowInstallation>) -> Self {
        Self {
            configured_installations,
            common_installation_roots: common_installation_roots(),
            battle_net_product_databases: battle_net_product_databases(),
            battle_net_configuration_files: battle_net_configuration_files(),
            registry_installation_roots: registry_installation_roots(),
        }
    }
}

/// Builds discovery results without modifying any candidate directory.
pub fn discover(inputs: &DiscoveryInputs) -> Vec<WowInstallationCandidate> {
    let mut candidates = BTreeMap::new();

    for installation in &inputs.configured_installations {
        match validate_installation_path(&installation.path) {
            Ok(validated) => insert_candidate_and_siblings(
                &mut candidates,
                WowInstallationCandidate {
                    id: installation.id.clone(),
                    // Product identity comes from the validated directory name.
                    // A custom path must never inherit a stale product hint and
                    // manufacture sibling candidates.
                    product: validated.product,
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

    for configuration_path in &inputs.battle_net_configuration_files {
        for root in battle_net_configuration_roots(configuration_path) {
            collect_root_and_product_children(
                &mut candidates,
                &root,
                WowDiscoverySource::BattleNet,
            );
        }
    }

    for root in &inputs.registry_installation_roots {
        collect_root_and_product_children(&mut candidates, root, WowDiscoverySource::BattleNet);
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

/// Resolves the directory selected in the native picker without guessing which
/// product a user intended. A direct product directory is accepted. A Battle.net
/// root is accepted only when it contains exactly one valid supported product.
pub fn resolve_installation_selection(
    path: &Path,
) -> Result<InstallationSelection, WowValidationError> {
    match validate_installation_path(path) {
        Ok(installation) => return Ok(InstallationSelection::Exact(installation)),
        Err(error) if product_for_path(path).is_some() => return Err(error),
        Err(_) => {}
    }

    let metadata = fs::metadata(path).map_err(|source| WowValidationError::InspectPath {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_dir() {
        return Err(WowValidationError::NotDirectory(path.to_path_buf()));
    }

    let mut valid_products = Vec::new();
    let mut invalid_products = Vec::new();
    for (directory_name, product) in PRODUCT_DIRECTORIES {
        let product_path = path.join(directory_name);
        let Ok(product_metadata) = fs::metadata(&product_path) else {
            continue;
        };
        if !product_metadata.is_dir() {
            continue;
        }

        match validate_installation_path(&product_path) {
            Ok(installation) => valid_products.push(installation),
            Err(error) => invalid_products.push((product, error)),
        }
    }

    match valid_products.len() {
        1 => Ok(InstallationSelection::Exact(valid_products.remove(0))),
        count if count > 1 => {
            valid_products.sort_by_key(|installation| product_priority(installation.product));
            Ok(InstallationSelection::MultipleProducts(valid_products))
        }
        _ if invalid_products.is_empty() => Err(WowValidationError::NoSupportedProductDirectories(
            path.to_path_buf(),
        )),
        _ => Err(WowValidationError::InvalidProductDirectories {
            root: path.to_path_buf(),
            errors: invalid_products,
        }),
    }
}

fn product_priority(product: Option<WowProduct>) -> u8 {
    match product {
        Some(WowProduct::Retail) => 0,
        Some(WowProduct::Ptr) => 1,
        Some(WowProduct::Beta) => 2,
        None => 3,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallationSelection {
    Exact(ValidatedWowInstallation),
    MultipleProducts(Vec<ValidatedWowInstallation>),
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

fn insert_candidate_and_siblings(
    candidates: &mut BTreeMap<PathBuf, WowInstallationCandidate>,
    candidate: WowInstallationCandidate,
) {
    let sibling_root = candidate
        .product
        .and_then(|_| candidate.path.parent().map(Path::to_path_buf));
    let source = candidate.source;
    insert_candidate(candidates, candidate);

    let Some(root) = sibling_root else {
        return;
    };
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
            insert_candidate_and_siblings(
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
        insert_candidate_and_siblings(
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

fn battle_net_configuration_files() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for variable in ["APPDATA", "LOCALAPPDATA"] {
        if let Some(directory) = env::var_os(variable) {
            paths.push(
                PathBuf::from(directory)
                    .join("Battle.net")
                    .join("Battle.net.config"),
            );
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn battle_net_installation_roots(database_path: &Path) -> Vec<PathBuf> {
    let Ok(contents) = fs::read(database_path) else {
        return Vec::new();
    };

    let mut roots = Vec::new();
    if let Ok(document) = serde_json::from_slice::<Value>(&contents) {
        collect_base_directories(&document, &mut roots);
    }
    roots.extend(extract_world_of_warcraft_paths(&contents));
    roots.sort();
    roots.dedup();
    roots
}

fn collect_base_directories(value: &Value, roots: &mut Vec<PathBuf>) {
    match value {
        Value::Object(object) => {
            for key in ["baseDir", "installPath", "InstallPath"] {
                if let Some(Value::String(path)) = object.get(key) {
                    roots.push(PathBuf::from(path));
                }
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

fn battle_net_configuration_roots(configuration_path: &Path) -> Vec<PathBuf> {
    let Ok(contents) = fs::read_to_string(configuration_path) else {
        return Vec::new();
    };
    let Ok(document) = serde_json::from_str::<Value>(&contents) else {
        return Vec::new();
    };

    let mut default_install_paths = Vec::new();
    collect_named_paths(&document, "DefaultInstallPath", &mut default_install_paths);
    let mut roots = Vec::new();
    for path in default_install_paths {
        let path = PathBuf::from(path);
        roots.push(path.join("World of Warcraft"));
        roots.push(path);
    }
    roots.sort();
    roots.dedup();
    roots
}

fn collect_named_paths(value: &Value, expected_name: &str, paths: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (name, child) in object {
                if name.eq_ignore_ascii_case(expected_name) {
                    if let Value::String(path) = child {
                        paths.push(path.clone());
                    }
                }
                collect_named_paths(child, expected_name, paths);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_named_paths(child, expected_name, paths);
            }
        }
        _ => {}
    }
}

/// Battle.net's Agent database is protobuf, not JSON. Its installation paths
/// are UTF-8 string fields, so extract only printable strings that contain the
/// game name and let structural validation decide whether they are usable.
fn extract_world_of_warcraft_paths(bytes: &[u8]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut start = None;
    for index in 0..=bytes.len() {
        let printable = index < bytes.len() && matches!(bytes[index], b' '..=b'~');
        match (start, printable) {
            (None, true) => start = Some(index),
            (Some(string_start), false) => {
                let value = String::from_utf8_lossy(&bytes[string_start..index]);
                if let Some(path) = path_from_battle_net_string(&value) {
                    paths.push(path);
                }
                start = None;
            }
            _ => {}
        }
    }
    paths
}

fn path_from_battle_net_string(value: &str) -> Option<PathBuf> {
    let lower = value.to_ascii_lowercase();
    let marker = "world of warcraft";
    let marker_index = lower.find(marker)?;
    let prefix = &value[..marker_index];
    let path_start = prefix
        .char_indices()
        .filter_map(|(index, _)| {
            let remainder = &prefix[index..];
            let bytes = remainder.as_bytes();
            (bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && matches!(bytes[2], b'\\' | b'/'))
            .then_some(index)
        })
        .next_back()?;
    let path_end = value[marker_index..]
        .find(['\"', '\'', '\0'])
        .map(|offset| marker_index + offset)
        .unwrap_or(value.len());
    Some(PathBuf::from(
        value[path_start..path_end].trim_end_matches([' ', '\\', '/']),
    ))
}

#[cfg(windows)]
fn registry_installation_roots() -> Vec<PathBuf> {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut roots = Vec::new();
    for key_name in [
        r"SOFTWARE\Blizzard Entertainment\World of Warcraft",
        r"SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft",
    ] {
        if let Ok(key) = hklm.open_subkey(key_name) {
            if let Ok(path) = key.get_value::<String, _>("InstallPath") {
                roots.push(PathBuf::from(path));
            }
        }
    }
    for base_key in [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        let Ok(parent) = hklm.open_subkey(base_key) else {
            continue;
        };
        for subkey_name in parent.enum_keys().flatten() {
            let Ok(key) = parent.open_subkey(subkey_name) else {
                continue;
            };
            let display_name = key
                .get_value::<String, _>("DisplayName")
                .unwrap_or_default();
            if display_name
                .to_ascii_lowercase()
                .contains("world of warcraft")
            {
                if let Ok(path) = key.get_value::<String, _>("InstallLocation") {
                    roots.push(PathBuf::from(path));
                }
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

#[cfg(not(windows))]
fn registry_installation_roots() -> Vec<PathBuf> {
    Vec::new()
}

#[derive(Debug)]
pub enum WowValidationError {
    InspectPath {
        path: PathBuf,
        source: io::Error,
    },
    InvalidProductDirectories {
        root: PathBuf,
        errors: Vec<(WowProduct, WowValidationError)>,
    },
    MissingExecutable(PathBuf),
    NoSupportedProductDirectories(PathBuf),
    NotDirectory(PathBuf),
}

impl fmt::Display for WowValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InspectPath { path, source } => {
                write!(formatter, "Could not inspect {}: {source}", path.display())
            }
            Self::InvalidProductDirectories { root, errors } => {
                write!(
                    formatter,
                    "Could not validate World of Warcraft product directories under {}",
                    root.display()
                )?;
                for (product, error) in errors {
                    write!(formatter, "; {product:?}: {error}")?;
                }
                Ok(())
            }
            Self::MissingExecutable(path) => write!(
                formatter,
                "Expected Wow.exe or Wow-64.exe in {}",
                path.display()
            ),
            Self::NoSupportedProductDirectories(path) => write!(
                formatter,
                "No supported WoW product directories were found under {}",
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
        fs::create_dir_all(root).expect("create installation directory");
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

    fn discovery_from_configured(path: PathBuf) -> Vec<WowInstallationCandidate> {
        discover(&DiscoveryInputs {
            configured_installations: vec![configured(path)],
            common_installation_roots: Vec::new(),
            battle_net_product_databases: Vec::new(),
            battle_net_configuration_files: Vec::new(),
            registry_installation_roots: Vec::new(),
        })
    }

    fn candidate(product: Option<WowProduct>, id: &str) -> WowInstallationCandidate {
        WowInstallationCandidate {
            id: id.to_owned(),
            product,
            path: PathBuf::from(format!("C:/Games/{id}")),
            availability: InstallationAvailability::Available,
            source: WowDiscoverySource::BattleNet,
            unavailable_reason: None,
        }
    }

    #[test]
    fn defaults_to_the_highest_priority_available_product() {
        let retail = candidate(Some(WowProduct::Retail), "retail");
        let ptr = candidate(Some(WowProduct::Ptr), "ptr");
        let beta = candidate(Some(WowProduct::Beta), "beta");
        let custom = candidate(None, "custom");

        assert_eq!(
            default_installation_candidate(&[retail.clone()]).map(|item| item.id.as_str()),
            Some("retail")
        );
        assert_eq!(
            default_installation_candidate(&[ptr.clone()]).map(|item| item.id.as_str()),
            Some("ptr")
        );
        assert_eq!(
            default_installation_candidate(&[retail.clone(), ptr])
                .map(|item| item.id.as_str()),
            Some("retail")
        );
        assert_eq!(
            default_installation_candidate(&[retail, beta, custom]).map(|item| item.id.as_str()),
            Some("retail")
        );
    }

    #[test]
    fn known_retail_discovers_valid_ptr_and_beta_siblings() {
        let directory = test_directory("retail-siblings");
        let root = directory.join("World of Warcraft");
        let retail = root.join("_retail_");
        wow_installation(&retail);
        wow_installation(&root.join("_ptr_"));
        wow_installation(&root.join("_beta_"));

        let candidates = discovery_from_configured(retail);

        assert_eq!(candidates.len(), 3);
        assert!(candidates.iter().any(|item| item.product == Some(WowProduct::Ptr)));
        assert!(candidates.iter().any(|item| item.product == Some(WowProduct::Beta)));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn known_ptr_discovers_retail_but_ignores_invalid_siblings() {
        let directory = test_directory("ptr-siblings");
        let root = directory.join("World of Warcraft");
        let ptr = root.join("_ptr_");
        wow_installation(&ptr);
        wow_installation(&root.join("_retail_"));
        fs::create_dir_all(root.join("_beta_")).expect("create invalid beta directory");

        let candidates = discovery_from_configured(ptr);

        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().any(|item| item.product == Some(WowProduct::Retail)));
        assert!(!candidates.iter().any(|item| item.product == Some(WowProduct::Beta)));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn sibling_candidates_are_deduplicated_and_custom_paths_do_not_expand() {
        let directory = test_directory("sibling-deduplication");
        let root = directory.join("World of Warcraft");
        let retail = root.join("_retail_");
        wow_installation(&retail);
        wow_installation(&root.join("_ptr_"));
        let mut inputs = DiscoveryInputs {
            configured_installations: vec![configured(retail)],
            common_installation_roots: vec![root],
            battle_net_product_databases: Vec::new(),
            battle_net_configuration_files: Vec::new(),
            registry_installation_roots: Vec::new(),
        };
        assert_eq!(discover(&inputs).len(), 2);

        let custom = directory.join("custom-wow");
        wow_installation(&custom);
        wow_installation(&directory.join("_ptr_"));
        inputs.configured_installations = vec![configured(custom)];
        inputs.common_installation_roots.clear();
        assert_eq!(discover(&inputs).len(), 1);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn validates_a_standard_retail_structure() {
        let directory = test_directory("retail");
        let retail = directory.join("_retail_");
        wow_installation(&retail);

        let validated = validate_installation_path(&retail).expect("validate retail fixture");
        let selection = resolve_installation_selection(&retail).expect("resolve retail fixture");

        assert_eq!(validated.product, Some(WowProduct::Retail));
        assert!(
            matches!(selection, InstallationSelection::Exact(resolved) if resolved == validated)
        );
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

        assert!(error.to_string().contains("Expected Wow.exe or Wow-64.exe"));
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
            battle_net_configuration_files: Vec::new(),
            registry_installation_roots: Vec::new(),
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
            battle_net_configuration_files: Vec::new(),
            registry_installation_roots: Vec::new(),
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

    #[test]
    fn resolves_a_wow_root_with_one_product_without_rpengine() {
        let directory = test_directory("single-product-root");
        let root = directory.join("World of Warcraft");
        let retail = root.join("_retail_");
        wow_installation(&retail);

        let selection = resolve_installation_selection(&root).expect("resolve root");

        assert_eq!(
            selection,
            InstallationSelection::Exact(ValidatedWowInstallation {
                product: Some(WowProduct::Retail),
                path: retail,
            })
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn returns_multiple_valid_products_without_choosing_one() {
        let directory = test_directory("multiple-product-root");
        let root = directory.join("World of Warcraft");
        wow_installation(&root.join("_retail_"));
        wow_installation(&root.join("_ptr_"));

        let selection = resolve_installation_selection(&root).expect("resolve root");

        assert!(matches!(
            selection,
            InstallationSelection::MultipleProducts(products)
                if products.len() == 2
                    && products.first().is_some_and(|product| product.product == Some(WowProduct::Retail))
                    && products.iter().any(|product| product.product == Some(WowProduct::Ptr))
        ));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn direct_product_missing_executable_keeps_the_specific_error() {
        let directory = test_directory("missing-executable");
        let product = directory.join("_retail_");
        fs::create_dir_all(&product).expect("create product directory");

        let error =
            resolve_installation_selection(&product).expect_err("reject missing executable");

        assert!(error.to_string().contains("Expected Wow.exe or Wow-64.exe"));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn direct_product_without_a_data_directory_is_valid_when_executable_exists() {
        let directory = test_directory("without-data");
        let product = directory.join("_retail_");
        fs::create_dir_all(&product).expect("create product directory");
        fs::write(product.join("Wow.exe"), "test executable").expect("create executable");

        let selection = resolve_installation_selection(&product)
            .expect("validate an executable without requiring a Data directory");

        assert!(matches!(selection, InstallationSelection::Exact(_)));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn rejects_an_arbitrary_selected_root_with_an_explicit_reason() {
        let directory = test_directory("arbitrary-root");

        let error = resolve_installation_selection(&directory).expect_err("reject arbitrary root");

        assert!(error
            .to_string()
            .contains("No supported WoW product directories were found"));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn reads_world_of_warcraft_paths_from_binary_product_database_strings() {
        let directory = test_directory("binary-product-db");
        let root = directory.join("World of Warcraft");
        wow_installation(&root.join("_retail_"));
        let database = directory.join("product.db");
        fs::write(
            &database,
            [
                b"metadata\x01".as_slice(),
                root.to_string_lossy().as_bytes(),
                b"\x02",
            ]
            .concat(),
        )
        .expect("write binary product database");

        let roots = battle_net_installation_roots(&database);

        assert_eq!(roots, vec![root]);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn reads_default_install_path_from_battle_net_configuration() {
        let directory = test_directory("battle-net-config");
        let install_root = directory.join("Games");
        let wow_root = install_root.join("World of Warcraft");
        let configuration = directory.join("Battle.net.config");
        fs::write(
            &configuration,
            serde_json::json!({ "Client": { "Install": { "DefaultInstallPath": install_root } } })
                .to_string(),
        )
        .expect("write Battle.net configuration");

        let roots = battle_net_configuration_roots(&configuration);

        assert!(roots.contains(&wow_root));
        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
