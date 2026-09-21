//! Read-only RPEngine addon inspection.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const ADDON_DIRECTORY: &str = "Interface/AddOns/RPEngine2";
const TOC_FILE: &str = "RPEngine2.toc";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RPEngineInstallation {
    pub status: RPEngineInstallationStatus,
    pub version: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RPEngineInstallationStatus {
    NotInstalled,
    Installed,
    Damaged,
    VersionUnavailable,
    /// Reserved for a later compatibility policy. Discovery does not infer an
    /// unsupported version without an explicit, trustworthy policy.
    Unsupported,
}

pub fn discover_rpengine(
    installation_path: &Path,
) -> Result<RPEngineInstallation, RPEngineDiscoveryError> {
    let addon_path = installation_path.join(ADDON_DIRECTORY);
    let metadata = match fs::metadata(&addon_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(not_installed()),
        Err(source) => {
            return Err(RPEngineDiscoveryError::Inspect {
                path: addon_path,
                source,
            })
        }
    };
    if !metadata.is_dir() {
        return Ok(damaged(format!(
            "{} exists but is not a directory.",
            addon_path.display()
        )));
    }

    let toc_path = addon_path.join(TOC_FILE);
    let toc_metadata = match fs::metadata(&toc_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(damaged(format!(
                "Expected addon metadata {} is missing.",
                toc_path.display()
            )));
        }
        Err(source) => {
            return Err(RPEngineDiscoveryError::Inspect {
                path: toc_path,
                source,
            })
        }
    };
    if !toc_metadata.is_file() {
        return Ok(damaged(format!(
            "Addon metadata {} is not a file.",
            toc_path.display()
        )));
    }

    let contents =
        fs::read_to_string(&toc_path).map_err(|source| RPEngineDiscoveryError::Read {
            path: toc_path,
            source,
        })?;
    match parse_version(&contents) {
        Some(version) => Ok(RPEngineInstallation {
            status: RPEngineInstallationStatus::Installed,
            version: Some(version),
            detail: None,
        }),
        None => Ok(RPEngineInstallation {
            status: RPEngineInstallationStatus::VersionUnavailable,
            version: None,
            detail: Some("RPEngine2.toc does not contain a usable Version field.".to_owned()),
        }),
    }
}

fn not_installed() -> RPEngineInstallation {
    RPEngineInstallation {
        status: RPEngineInstallationStatus::NotInstalled,
        version: None,
        detail: None,
    }
}

fn damaged(detail: String) -> RPEngineInstallation {
    RPEngineInstallation {
        status: RPEngineInstallationStatus::Damaged,
        version: None,
        detail: Some(detail),
    }
}

fn parse_version(contents: &str) -> Option<String> {
    for line in contents.lines() {
        let Some(metadata) = line.trim().strip_prefix("##") else {
            continue;
        };
        let Some((name, value)) = metadata.trim().split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("Version") {
            let version = value.trim();
            if !version.is_empty()
                && !version.contains('@')
                && !version.chars().any(char::is_control)
            {
                return Some(version.to_owned());
            }
            return None;
        }
    }
    None
}

#[derive(Debug)]
pub enum RPEngineDiscoveryError {
    Inspect { path: PathBuf, source: io::Error },
    Read { path: PathBuf, source: io::Error },
}

impl fmt::Display for RPEngineDiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inspect { path, source } => {
                write!(formatter, "Could not inspect {}: {source}", path.display())
            }
            Self::Read { path, source } => {
                write!(formatter, "Could not read {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for RPEngineDiscoveryError {}

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
            .expect("clock")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("rpengine-manager-rpengine-{name}-{unique}"));
        fs::create_dir_all(&directory).expect("create fixture directory");
        directory
    }

    #[test]
    fn reports_absent_addon() {
        let directory = test_directory("absent");
        let result = discover_rpengine(&directory).expect("discover addon");

        assert_eq!(result.status, RPEngineInstallationStatus::NotInstalled);
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }

    #[test]
    fn reports_readable_installed_version() {
        let directory = test_directory("valid");
        let toc = directory.join(ADDON_DIRECTORY).join(TOC_FILE);
        fs::create_dir_all(toc.parent().expect("toc parent")).expect("create addon directory");
        fs::write(&toc, "## Interface: 110000\n## Version: 2.0.alpha5\n").expect("write toc");

        let result = discover_rpengine(&directory).expect("discover addon");

        assert_eq!(result.status, RPEngineInstallationStatus::Installed);
        assert_eq!(result.version.as_deref(), Some("2.0.alpha5"));
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }

    #[test]
    fn reports_missing_or_invalid_metadata_without_calling_the_addon_healthy() {
        let directory = test_directory("damaged");
        let addon = directory.join(ADDON_DIRECTORY);
        fs::create_dir_all(&addon).expect("create addon directory");

        let missing_toc = discover_rpengine(&directory).expect("discover missing toc");
        assert_eq!(missing_toc.status, RPEngineInstallationStatus::Damaged);

        fs::write(
            addon.join(TOC_FILE),
            "## Interface: 110000\n## Version: @project-version@\n",
        )
        .expect("write invalid toc");
        let invalid_toc = discover_rpengine(&directory).expect("discover invalid toc");
        assert_eq!(
            invalid_toc.status,
            RPEngineInstallationStatus::VersionUnavailable
        );
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }
}
