//! Read-only account directory discovery for one WoW installation.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const ACCOUNT_DIRECTORY: &str = "WTF/Account";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WowAccount {
    pub id: String,
    pub path: PathBuf,
}

/// Lists immediate account directories only. Character data is never opened.
pub fn discover_accounts(
    installation_path: &Path,
) -> Result<Vec<WowAccount>, AccountDiscoveryError> {
    let account_path = installation_path.join(ACCOUNT_DIRECTORY);
    let metadata = match fs::metadata(&account_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(AccountDiscoveryError::Inspect {
                path: account_path,
                source,
            });
        }
    };
    if !metadata.is_dir() {
        return Err(AccountDiscoveryError::NotDirectory(account_path));
    }

    let entries = fs::read_dir(&account_path).map_err(|source| AccountDiscoveryError::Read {
        path: account_path.clone(),
        source,
    })?;
    let mut accounts = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| AccountDiscoveryError::Read {
            path: account_path.clone(),
            source,
        })?;
        let file_type = entry
            .file_type()
            .map_err(|source| AccountDiscoveryError::Inspect {
                path: entry.path(),
                source,
            })?;
        if file_type.is_dir() {
            let id = entry.file_name().to_string_lossy().into_owned();
            if !id.trim().is_empty() {
                accounts.push(WowAccount {
                    id,
                    path: entry.path(),
                });
            }
        }
    }
    accounts.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(accounts)
}

#[derive(Debug)]
pub enum AccountDiscoveryError {
    Inspect { path: PathBuf, source: io::Error },
    NotDirectory(PathBuf),
    Read { path: PathBuf, source: io::Error },
}

impl fmt::Display for AccountDiscoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inspect { path, source } => {
                write!(formatter, "Could not inspect {}: {source}", path.display())
            }
            Self::NotDirectory(path) => {
                write!(formatter, "{} is not an account directory", path.display())
            }
            Self::Read { path, source } => {
                write!(formatter, "Could not read {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for AccountDiscoveryError {}

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
            std::env::temp_dir().join(format!("rpengine-manager-accounts-{name}-{unique}"));
        fs::create_dir_all(&directory).expect("create fixture directory");
        directory
    }

    #[test]
    fn returns_no_accounts_when_the_account_directory_is_absent() {
        let directory = test_directory("none");
        assert!(discover_accounts(&directory)
            .expect("discover accounts")
            .is_empty());
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }

    #[test]
    fn returns_one_account_directory() {
        let directory = test_directory("one");
        fs::create_dir_all(directory.join(ACCOUNT_DIRECTORY).join("ACCOUNT_ONE"))
            .expect("create account");

        let accounts = discover_accounts(&directory).expect("discover accounts");

        assert_eq!(
            accounts
                .iter()
                .map(|account| account.id.as_str())
                .collect::<Vec<_>>(),
            ["ACCOUNT_ONE"]
        );
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }

    #[test]
    fn returns_multiple_account_directories_and_ignores_files() {
        let directory = test_directory("multiple");
        let accounts_path = directory.join(ACCOUNT_DIRECTORY);
        fs::create_dir_all(accounts_path.join("ACCOUNT_B")).expect("create account B");
        fs::create_dir_all(accounts_path.join("ACCOUNT_A")).expect("create account A");
        fs::write(accounts_path.join("notes.txt"), "not an account")
            .expect("create non-account file");

        let accounts = discover_accounts(&directory).expect("discover accounts");

        assert_eq!(
            accounts
                .iter()
                .map(|account| account.id.as_str())
                .collect::<Vec<_>>(),
            ["ACCOUNT_A", "ACCOUNT_B"]
        );
        fs::remove_dir_all(directory).expect("remove fixture directory");
    }
}
