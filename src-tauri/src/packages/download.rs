//! Secure retrieval and verification of a single catalogue package revision.

use std::{fmt, str};

use sha2::{Digest, Sha256};

use crate::catalogue::{CatalogueClient, CatalogueError, RevisionDetailResponse};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPackagePayload {
    pub catalogue_id: String,
    pub package_type: String,
    pub native_rpe_id: String,
    pub revision: u64,
    pub sha256: String,
    pub payload: String,
}

pub struct PackageDownloadService {
    client: CatalogueClient,
}

impl PackageDownloadService {
    pub fn production() -> Result<Self, PackageDownloadError> {
        Ok(Self {
            client: CatalogueClient::production()?,
        })
    }
    pub fn new(client: CatalogueClient) -> Self {
        Self { client }
    }

    pub fn retrieve(
        &self,
        catalogue_id: &str,
        revision: u64,
        password: Option<&str>,
    ) -> Result<ValidatedPackagePayload, PackageDownloadError> {
        let detail = self.client.get_revision(catalogue_id, revision)?;
        validate_metadata(&detail, catalogue_id, revision)?;
        let token = if detail.package.protected {
            let password = password
                .filter(|password| !password.is_empty())
                .ok_or(PackageDownloadError::MissingPassword)?;
            Some(
                self.client
                    .unlock(catalogue_id, password)
                    .map_err(PackageDownloadError::unlock)?,
            )
        } else {
            None
        };
        let response = self
            .client
            .download_payload(catalogue_id, revision, token.as_deref())
            .map_err(PackageDownloadError::download)?;
        validate_headers(&response.headers, &detail, catalogue_id, revision)?;
        if response.bytes.len() as u64 != detail.revision.payload_byte_size {
            return Err(PackageDownloadError::ByteSizeMismatch);
        }
        let actual_hash = format!("{:x}", Sha256::digest(&response.bytes));
        if !detail.revision.sha256.eq_ignore_ascii_case(&actual_hash) {
            return Err(PackageDownloadError::ShaMismatch);
        }
        if let Some(declared_hash) = header(&response.headers, "X-RPE-SHA256") {
            if !declared_hash.eq_ignore_ascii_case(&actual_hash) {
                return Err(PackageDownloadError::ShaMismatch);
            }
        }
        let payload = str::from_utf8(&response.bytes)
            .map_err(|_| PackageDownloadError::NonUtf8Payload)?
            .to_owned();
        Ok(ValidatedPackagePayload {
            catalogue_id: detail.package.catalogue_id,
            package_type: detail.package.package_type,
            native_rpe_id: detail.package.native_rpe_id,
            revision,
            sha256: actual_hash,
            payload,
        })
    }

    /// Resolves the catalogue's current revision when the caller did not pin
    /// one, then follows the same verified retrieval path.
    pub fn retrieve_selected(
        &self,
        catalogue_id: &str,
        revision: Option<u64>,
        password: Option<&str>,
    ) -> Result<ValidatedPackagePayload, PackageDownloadError> {
        let revision = match revision {
            Some(revision) => revision,
            None => {
                self.client
                    .get_package(catalogue_id)?
                    .current_revision
                    .revision
            }
        };
        self.retrieve(catalogue_id, revision, password)
    }
}

fn validate_metadata(
    detail: &RevisionDetailResponse,
    catalogue_id: &str,
    revision: u64,
) -> Result<(), PackageDownloadError> {
    if detail.package.catalogue_id != catalogue_id
        || detail.revision.catalogue_id != catalogue_id
        || detail.revision.revision != revision
        || detail.package.native_rpe_id.is_empty()
    {
        return Err(PackageDownloadError::IncompatibleResponse(
            "revision metadata identity does not match request".into(),
        ));
    }
    Ok(())
}
fn header<'a>(headers: &'a reqwest::header::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}
fn validate_headers(
    headers: &reqwest::header::HeaderMap,
    detail: &RevisionDetailResponse,
    catalogue_id: &str,
    revision: u64,
) -> Result<(), PackageDownloadError> {
    for (name, expected) in [
        ("X-RPE-Catalogue-ID", catalogue_id),
        ("X-RPE-Package-Type", detail.package.package_type.as_str()),
        ("X-RPE-Native-ID", detail.package.native_rpe_id.as_str()),
    ] {
        if let Some(actual) = header(headers, name) {
            if actual != expected {
                return Err(PackageDownloadError::HeaderMismatch(name.into()));
            }
        }
    }
    if let Some(actual) = header(headers, "X-RPE-Revision") {
        if actual.parse::<u64>().ok() != Some(revision) {
            return Err(PackageDownloadError::HeaderMismatch(
                "X-RPE-Revision".into(),
            ));
        }
    }
    if let Some(actual) = header(headers, "X-RPE-Byte-Size") {
        if actual.parse::<u64>().ok() != Some(detail.revision.payload_byte_size) {
            return Err(PackageDownloadError::ByteSizeMismatch);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageDownloadError {
    MissingPassword,
    InvalidPassword,
    AuthorizationFailed,
    PackageNotFound,
    IncompatibleResponse(String),
    NonUtf8Payload,
    ShaMismatch,
    ByteSizeMismatch,
    HeaderMismatch(String),
    Network(String),
    Timeout(String),
    Remote(String),
}
impl PackageDownloadError {
    fn unlock(error: CatalogueError) -> Self {
        match error {
            CatalogueError::Http {
                status: 401 | 403, ..
            } => Self::InvalidPassword,
            error => Self::from(error),
        }
    }
    fn download(error: CatalogueError) -> Self {
        match error {
            CatalogueError::Http {
                status: 401 | 403, ..
            } => Self::AuthorizationFailed,
            error => Self::from(error),
        }
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingPassword => "missing_password",
            Self::InvalidPassword => "invalid_password",
            Self::AuthorizationFailed => "authorization_failed",
            Self::PackageNotFound => "package_not_found",
            Self::IncompatibleResponse(_) => "incompatible_package_response",
            Self::NonUtf8Payload => "non_utf8_payload",
            Self::ShaMismatch => "sha_mismatch",
            Self::ByteSizeMismatch => "byte_size_mismatch",
            Self::HeaderMismatch(_) => "identity_header_mismatch",
            Self::Network(_) => "network_unavailable",
            Self::Timeout(_) => "timeout",
            Self::Remote(_) => "remote_failure",
        }
    }
}
impl From<CatalogueError> for PackageDownloadError {
    fn from(error: CatalogueError) -> Self {
        match error {
            CatalogueError::Http { status: 404, .. } => Self::PackageNotFound,
            CatalogueError::Network(message) => Self::Network(message),
            CatalogueError::Timeout(message) => Self::Timeout(message),
            CatalogueError::Http { .. }
            | CatalogueError::MalformedResponse(_)
            | CatalogueError::InvalidBaseUrl(_)
            | CatalogueError::UnsupportedResponse(_) => {
                Self::IncompatibleResponse(error.to_string())
            }
        }
    }
}
impl fmt::Display for PackageDownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPassword => {
                f.write_str("a password is required for this protected package")
            }
            Self::InvalidPassword => f.write_str("the package password was rejected"),
            Self::AuthorizationFailed => {
                f.write_str("package download authorization was rejected or expired")
            }
            Self::PackageNotFound => f.write_str("package or revision was not found"),
            Self::IncompatibleResponse(message)
            | Self::Network(message)
            | Self::Timeout(message)
            | Self::Remote(message) => f.write_str(message),
            Self::NonUtf8Payload => f.write_str("package payload is not valid UTF-8"),
            Self::ShaMismatch => {
                f.write_str("downloaded package SHA-256 does not match its revision")
            }
            Self::ByteSizeMismatch => {
                f.write_str("downloaded package byte size does not match its revision")
            }
            Self::HeaderMismatch(name) => write!(
                f,
                "download identity header {name} does not match its revision"
            ),
        }
    }
}
impl std::error::Error for PackageDownloadError {}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };

    use super::*;

    type MockResponse = (u16, Vec<(&'static str, String)>, Vec<u8>);

    fn sha(payload: &[u8]) -> String {
        format!("{:x}", Sha256::digest(payload))
    }
    fn detail(payload: &[u8], protected: bool, revision: u64) -> String {
        format!(
            r#"{{"package":{{"catalogueId":"cat.one","publisherId":"p","publisherType":"player","publisherDisplayName":"P","packageType":"dataset","nativeRpeId":"native.one","datasetType":null,"datasetGroup":null,"name":"One","shortDescription":"","fullDescription":"","currentRevision":{revision},"publicationState":"published","protected":{protected},"createdAt":"x","updatedAt":"x"}},"revision":{{"catalogueId":"cat.one","revision":{revision},"sha256":"{}","payloadByteSize":{},"changelog":"","publishedAt":"x","minimumRpeVersion":null,"maximumRpeVersion":null,"requiredManagerProtocolVersion":null}},"dependencies":[]}}"#,
            sha(payload),
            payload.len()
        )
    }
    fn server(responses: Vec<MockResponse>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        thread::spawn(move || {
            for (status, headers, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = [0; 4096];
                let count = stream.read(&mut bytes).unwrap();
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&bytes[..count]).into_owned());
                let mut response =
                    format!("HTTP/1.1 {status} OK\r\nContent-Length: {}\r\n", body.len());
                for (name, value) in headers {
                    response.push_str(&format!("{name}: {value}\r\n"));
                }
                response.push_str("Connection: close\r\n\r\n");
                stream.write_all(response.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        (format!("http://{address}/api/rpe/v1"), requests)
    }
    fn headers(payload: &[u8], revision: u64) -> Vec<(&'static str, String)> {
        vec![
            ("X-RPE-Catalogue-ID", "cat.one".into()),
            ("X-RPE-Package-Type", "dataset".into()),
            ("X-RPE-Native-ID", "native.one".into()),
            ("X-RPE-Revision", revision.to_string()),
            ("X-RPE-Byte-Size", payload.len().to_string()),
            ("X-RPE-SHA256", sha(payload)),
        ]
    }

    #[test]
    fn downloads_public_payload_without_rewriting_bytes() {
        let payload = b"RPE_DATASET_V1\n{ dataset = {} }";
        let (url, _) = server(vec![
            (200, vec![], detail(payload, false, 7).into_bytes()),
            (200, headers(payload, 7), payload.to_vec()),
        ]);
        let result = PackageDownloadService::new(CatalogueClient::new(&url).unwrap())
            .retrieve("cat.one", 7, None)
            .unwrap();
        assert_eq!(result.payload.as_bytes(), payload);
        assert_eq!(result.sha256, sha(payload));
    }
    #[test]
    fn unlocks_protected_payload_without_leaking_token() {
        let payload = b"RPE_DATASET_V1\nprotected";
        let (url, seen) = server(vec![
            (200, vec![], detail(payload, true, 7).into_bytes()),
            (200, vec![], br#"{"token":"very-secret-token"}"#.to_vec()),
            (200, headers(payload, 7), payload.to_vec()),
        ]);
        let result = PackageDownloadService::new(CatalogueClient::new(&url).unwrap())
            .retrieve("cat.one", 7, Some("password"))
            .unwrap();
        assert_eq!(result.payload.as_bytes(), payload);
        let requests = seen.lock().unwrap();
        assert!(requests[2]
            .to_ascii_lowercase()
            .contains("authorization: bearer very-secret-token"));
    }
    #[test]
    fn rejects_wrong_password_and_protected_download_authorization() {
        let payload = b"x";
        let (url, _) = server(vec![
            (200, vec![], detail(payload, true, 7).into_bytes()),
            (401, vec![], br#"{"error":{"message":"no"}}"#.to_vec()),
        ]);
        let error = PackageDownloadService::new(CatalogueClient::new(&url).unwrap())
            .retrieve("cat.one", 7, Some("bad"))
            .unwrap_err();
        assert_eq!(error, PackageDownloadError::InvalidPassword);
        assert!(!error.to_string().contains("bad"));
        let (url, _) = server(vec![
            (200, vec![], detail(payload, false, 7).into_bytes()),
            (403, vec![], Vec::new()),
        ]);
        let error = PackageDownloadService::new(CatalogueClient::new(&url).unwrap())
            .retrieve("cat.one", 7, None)
            .unwrap_err();
        assert_eq!(error, PackageDownloadError::AuthorizationFailed);
    }
    #[test]
    fn rejects_hash_byte_size_and_identity_mismatches() {
        let payload = b"payload";
        let (url, _) = server(vec![
            (200, vec![], detail(payload, false, 7).into_bytes()),
            (200, headers(b"wrong", 7), payload.to_vec()),
        ]);
        assert_eq!(
            PackageDownloadService::new(CatalogueClient::new(&url).unwrap())
                .retrieve("cat.one", 7, None)
                .unwrap_err(),
            PackageDownloadError::ByteSizeMismatch
        );
        let mut bad_headers = headers(payload, 7);
        bad_headers[0].1 = "other".into();
        let (url, _) = server(vec![
            (200, vec![], detail(payload, false, 7).into_bytes()),
            (200, bad_headers, payload.to_vec()),
        ]);
        assert!(matches!(
            PackageDownloadService::new(CatalogueClient::new(&url).unwrap())
                .retrieve("cat.one", 7, None),
            Err(PackageDownloadError::HeaderMismatch(_))
        ));
        let mut bad_hash = headers(payload, 7);
        bad_hash[5].1 = "00".into();
        let (url, _) = server(vec![
            (200, vec![], detail(payload, false, 7).into_bytes()),
            (200, bad_hash, payload.to_vec()),
        ]);
        assert_eq!(
            PackageDownloadService::new(CatalogueClient::new(&url).unwrap())
                .retrieve("cat.one", 7, None)
                .unwrap_err(),
            PackageDownloadError::ShaMismatch
        );
        let (url, _) = server(vec![(200, vec![], detail(payload, false, 8).into_bytes())]);
        assert!(matches!(
            PackageDownloadService::new(CatalogueClient::new(&url).unwrap())
                .retrieve("cat.one", 7, None),
            Err(PackageDownloadError::IncompatibleResponse(_))
        ));
    }
}
