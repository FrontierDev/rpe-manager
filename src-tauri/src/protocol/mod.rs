//! Typed protocol-v1 models for the RPEngine addon integration contract.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROTOCOL_VERSION: u32 = 1;
const MAX_REVISION: u32 = 2_147_483_647;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtocolState {
    pub protocol_version: u32,
    pub pending_operations: Vec<OperationEnvelope>,
    pub installed_packages: BTreeMap<String, InstalledPackage>,
    pub operation_results: BTreeMap<String, OperationResult>,
}

impl Default for ProtocolState {
    fn default() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            pending_operations: Vec::new(),
            installed_packages: BTreeMap::new(),
            operation_results: BTreeMap::new(),
        }
    }
}

impl ProtocolState {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolValidationError::UnsupportedProtocolVersion(
                self.protocol_version,
            ));
        }

        for operation in &self.pending_operations {
            operation.validate()?;
        }

        for (catalogue_id, package) in &self.installed_packages {
            validate_catalogue_id(catalogue_id)?;
            package.validate()?;
        }

        for (request_id, result) in &self.operation_results {
            validate_request_id(request_id)?;
            if request_id != &result.request_id {
                return Err(ProtocolValidationError::InvalidField {
                    field: "operationResults",
                    message: format!(
                        "map key {request_id:?} does not match result requestId {:?}",
                        result.request_id
                    ),
                });
            }
            result.validate()?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    InstallDataset,
    RemoveDataset,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationEnvelope {
    pub request_id: String,
    pub operation: OperationKind,
    pub catalogue_id: String,
    pub dataset_id: String,
    pub revision: u32,
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

impl OperationEnvelope {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        validate_request_id(&self.request_id)?;
        validate_catalogue_id(&self.catalogue_id)?;
        validate_dataset_id(&self.dataset_id)?;
        validate_revision(self.revision)?;
        validate_hash(&self.hash)?;

        match (self.operation, self.payload.as_deref()) {
            (OperationKind::InstallDataset, Some(payload)) if valid_payload_header(payload) => {
                Ok(())
            }
            (OperationKind::InstallDataset, _) => Err(ProtocolValidationError::InvalidField {
                field: "payload",
                message: "install_dataset requires a non-empty RPE_DATASET_V1 payload".to_owned(),
            }),
            (OperationKind::RemoveDataset, None) => Ok(()),
            (OperationKind::RemoveDataset, Some(_)) => Err(ProtocolValidationError::InvalidField {
                field: "payload",
                message: "remove_dataset must not contain a payload".to_owned(),
            }),
        }
    }

    /// Performs the Manager-only pre-queue verification. Persisted operations
    /// are still parsed with `validate`, because RPE owns their eventual
    /// canonical payload validation.
    pub fn validate_for_queue(&self) -> Result<(), ProtocolValidationError> {
        self.validate()?;
        if let (OperationKind::InstallDataset, Some(payload)) = (self.operation, &self.payload) {
            let actual = format!("{:x}", Sha256::digest(payload.as_bytes()));
            if actual != self.hash {
                return Err(ProtocolValidationError::InvalidField {
                    field: "hash",
                    message: "does not match the exact UTF-8 payload bytes".to_owned(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledPackage {
    pub package_type: PackageType,
    pub dataset_id: String,
    pub revision: u32,
    pub hash: String,
    pub installed_at: u64,
}

impl InstalledPackage {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        if self.package_type != PackageType::Dataset {
            return Err(ProtocolValidationError::InvalidField {
                field: "packageType",
                message: "protocol v1 only supports dataset packages".to_owned(),
            });
        }
        validate_dataset_id(&self.dataset_id)?;
        validate_revision(self.revision)?;
        validate_hash(&self.hash)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageType {
    Dataset,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationResult {
    pub request_id: String,
    pub operation: ResultOperation,
    pub status: OperationResultStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalogue_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtocolFailure>,
}

impl OperationResult {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        validate_request_id(&self.request_id)?;
        if let Some(catalogue_id) = &self.catalogue_id {
            validate_catalogue_id(catalogue_id)?;
        }
        if let Some(dataset_id) = &self.dataset_id {
            validate_dataset_id(dataset_id)?;
        }
        if let Some(revision) = self.revision {
            validate_revision(revision)?;
        }
        if let Some(hash) = &self.hash {
            validate_hash(hash)?;
        }

        match (self.status, self.error.as_ref()) {
            (OperationResultStatus::Succeeded, None) => {
                if self.operation == ResultOperation::Unknown
                    || self.catalogue_id.is_none()
                    || self.dataset_id.is_none()
                    || self.revision.is_none()
                    || self.hash.is_none()
                {
                    return Err(ProtocolValidationError::InvalidField {
                        field: "operationResults",
                        message: "successful results require a known operation and complete package identity".to_owned(),
                    });
                }
                Ok(())
            }
            (OperationResultStatus::Failed, Some(error)) => error.validate(),
            (OperationResultStatus::Succeeded, Some(_)) => {
                Err(ProtocolValidationError::InvalidField {
                    field: "error",
                    message: "successful results must not contain an error".to_owned(),
                })
            }
            (OperationResultStatus::Failed, None) => Err(ProtocolValidationError::InvalidField {
                field: "error",
                message: "failed results require an error".to_owned(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultOperation {
    InstallDataset,
    RemoveDataset,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationResultStatus {
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtocolFailure {
    pub code: ProtocolErrorCode,
    pub detail: String,
}

impl ProtocolFailure {
    pub fn validate(&self) -> Result<(), ProtocolValidationError> {
        if self.detail.trim().is_empty() || self.detail.len() > 1024 {
            return Err(ProtocolValidationError::InvalidField {
                field: "error.detail",
                message: "failure detail must contain 1 to 1024 non-whitespace UTF-8 bytes"
                    .to_owned(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolErrorCode {
    InvalidField,
    UnsupportedOperation,
    InvalidPayloadFormat,
    DatasetIdMismatch,
    CatalogueDatasetIdConflict,
    StaleRevision,
    RevisionHashConflict,
    PackageNotInstalled,
    InstalledPackageMismatch,
    ImportRejected,
    RemoveRejected,
    InternalError,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolValidationError {
    UnsupportedProtocolVersion(u32),
    InvalidField {
        field: &'static str,
        message: String,
    },
}

impl fmt::Display for ProtocolValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProtocolVersion(version) => {
                write!(
                    formatter,
                    "unsupported RPEngine Manager protocol version {version}"
                )
            }
            Self::InvalidField { field, message } => {
                write!(formatter, "invalid protocol field {field}: {message}")
            }
        }
    }
}

impl std::error::Error for ProtocolValidationError {}

pub fn validate_request_id(value: &str) -> Result<(), ProtocolValidationError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => true,
            b'.' | b'_' | b':' | b'-' => index > 0,
            _ => false,
        });
    if valid {
        Ok(())
    } else {
        Err(ProtocolValidationError::InvalidField {
            field: "requestId",
            message: "must match [A-Za-z0-9][A-Za-z0-9._:-]{0,127}".to_owned(),
        })
    }
}

pub fn validate_catalogue_id(value: &str) -> Result<(), ProtocolValidationError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'0'..=b'9' => true,
            b'.' | b'_' | b'-' => index > 0,
            _ => false,
        });
    if valid {
        Ok(())
    } else {
        Err(ProtocolValidationError::InvalidField {
            field: "catalogueId",
            message: "must match [a-z0-9][a-z0-9._-]{0,127}".to_owned(),
        })
    }
}

pub fn validate_dataset_id(value: &str) -> Result<(), ProtocolValidationError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && !value.trim().is_empty()
        && value.trim() == value
        && !value.chars().any(|character| character.is_control());
    if valid {
        Ok(())
    } else {
        Err(ProtocolValidationError::InvalidField {
            field: "datasetId",
            message:
                "must be 1 to 128 UTF-8 bytes without control characters or surrounding whitespace"
                    .to_owned(),
        })
    }
}

pub fn validate_revision(value: u32) -> Result<(), ProtocolValidationError> {
    if value > 0 && value <= MAX_REVISION {
        Ok(())
    } else {
        Err(ProtocolValidationError::InvalidField {
            field: "revision",
            message: "must be an integer from 1 through 2147483647".to_owned(),
        })
    }
}

pub fn validate_hash(value: &str) -> Result<(), ProtocolValidationError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ProtocolValidationError::InvalidField {
            field: "hash",
            message: "must be 64 lowercase hexadecimal SHA-256 characters".to_owned(),
        })
    }
}

pub fn valid_payload_header(payload: &str) -> bool {
    payload.starts_with("RPE_DATASET_V1\n")
}
