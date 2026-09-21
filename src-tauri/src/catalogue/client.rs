use std::{fmt, time::Duration};

use reqwest::{blocking::Client, header::HeaderMap, Url};
use serde::de::DeserializeOwned;

use super::{
    CataloguePackagesResponse, PackageDetailResponse, PublishersResponse, RevisionDetailResponse,
    RevisionsResponse,
};

pub const DEFAULT_BASE_URL: &str = "https://esarus.net/api/rpe/v1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct CatalogueClient {
    base_url: Url,
    http: Client,
}

impl CatalogueClient {
    pub fn production() -> Result<Self, CatalogueError> {
        Self::new(DEFAULT_BASE_URL)
    }

    pub fn new(base_url: &str) -> Result<Self, CatalogueError> {
        Self::with_timeout(base_url, REQUEST_TIMEOUT)
    }

    /// Allows deterministic local-server tests without coupling them to the
    /// production timeout policy.
    pub fn with_timeout(base_url: &str, timeout: Duration) -> Result<Self, CatalogueError> {
        let mut base_url = Url::parse(base_url)
            .map_err(|error| CatalogueError::InvalidBaseUrl(error.to_string()))?;
        if base_url.scheme() != "https" && base_url.scheme() != "http" {
            return Err(CatalogueError::InvalidBaseUrl(
                "base URL must use HTTP(S)".into(),
            ));
        }
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        let http = Client::builder()
            .timeout(timeout)
            .user_agent("RPEngine-Manager")
            .build()
            .map_err(|error| CatalogueError::Network(error.to_string()))?;
        Ok(Self { base_url, http })
    }

    pub fn get_catalogue_packages(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<CataloguePackagesResponse, CatalogueError> {
        let mut url = self.endpoint("catalogue")?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(value) = limit {
                query.append_pair("limit", &value.to_string());
            }
            if let Some(value) = offset {
                query.append_pair("offset", &value.to_string());
            }
        }
        self.get(url)
    }

    pub fn get_package(&self, catalogue_id: &str) -> Result<PackageDetailResponse, CatalogueError> {
        self.get(self.package_url(catalogue_id, "")?)
    }

    pub fn get_revisions(
        &self,
        catalogue_id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<RevisionsResponse, CatalogueError> {
        let mut url = self.package_url(catalogue_id, "revisions")?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(value) = limit {
                query.append_pair("limit", &value.to_string());
            }
            if let Some(value) = offset {
                query.append_pair("offset", &value.to_string());
            }
        }
        self.get(url)
    }

    pub fn get_revision(
        &self,
        catalogue_id: &str,
        revision: u64,
    ) -> Result<RevisionDetailResponse, CatalogueError> {
        self.get(self.package_url(catalogue_id, &format!("revisions/{revision}"))?)
    }

    pub fn get_publishers(&self) -> Result<PublishersResponse, CatalogueError> {
        self.get(self.endpoint("publishers")?)
    }

    /// Exchanges a protected package password for its short-lived scoped token.
    /// The token stays in the backend caller and is never serialized by this
    /// client or its errors.
    pub fn unlock(&self, catalogue_id: &str, password: &str) -> Result<String, CatalogueError> {
        let url = self.package_url(catalogue_id, "unlock")?;
        let response = self
            .http
            .post(url)
            .json(&serde_json::json!({ "password": password }))
            .send()
            .map_err(CatalogueError::request)?;
        let status = response.status();
        let body = response.text().map_err(CatalogueError::request)?;
        if !status.is_success() {
            return Err(CatalogueError::Http {
                status: status.as_u16(),
                server: server_message(&body),
            });
        }
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|error| CatalogueError::MalformedResponse(error.to_string()))?;
        ["token", "authorizationToken", "accessToken"]
            .iter()
            .find_map(|field| value.get(*field).and_then(|value| value.as_str()))
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                CatalogueError::UnsupportedResponse(
                    "unlock response has no authorization token".into(),
                )
            })
    }

    pub fn download_payload(
        &self,
        catalogue_id: &str,
        revision: u64,
        token: Option<&str>,
    ) -> Result<DownloadResponse, CatalogueError> {
        let url = self.package_url(catalogue_id, &format!("revisions/{revision}/download"))?;
        let mut request = self.http.get(url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(CatalogueError::request)?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().map_err(CatalogueError::request)?;
        if !status.is_success() {
            return Err(CatalogueError::Http {
                status: status.as_u16(),
                server: server_message(std::str::from_utf8(&body).unwrap_or_default()),
            });
        }
        Ok(DownloadResponse {
            bytes: body.to_vec(),
            headers,
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url, CatalogueError> {
        self.base_url
            .join(path)
            .map_err(|error| CatalogueError::InvalidBaseUrl(error.to_string()))
    }
    fn package_url(&self, catalogue_id: &str, suffix: &str) -> Result<Url, CatalogueError> {
        if catalogue_id.is_empty() || catalogue_id.contains('/') {
            return Err(CatalogueError::UnsupportedResponse(
                "invalid catalogue ID".into(),
            ));
        }
        let path = format!("packages/{catalogue_id}/{suffix}");
        self.endpoint(path.trim_end_matches('/'))
    }
    fn get<T: DeserializeOwned>(&self, url: Url) -> Result<T, CatalogueError> {
        let response = self.http.get(url).send().map_err(CatalogueError::request)?;
        let status = response.status();
        let body = response.text().map_err(CatalogueError::request)?;
        if !status.is_success() {
            return Err(CatalogueError::Http {
                status: status.as_u16(),
                server: server_message(&body),
            });
        }
        serde_json::from_str(&body)
            .map_err(|error| CatalogueError::MalformedResponse(error.to_string()))
    }
}

#[derive(Clone, Debug)]
pub struct DownloadResponse {
    pub bytes: Vec<u8>,
    pub headers: HeaderMap,
}

fn server_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = value.get("error").unwrap_or(&value);
    error
        .get("message")
        .or_else(|| error.get("code"))
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogueError {
    InvalidBaseUrl(String),
    Network(String),
    Timeout(String),
    Http { status: u16, server: Option<String> },
    MalformedResponse(String),
    UnsupportedResponse(String),
}
impl CatalogueError {
    fn request(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout(error.to_string())
        } else {
            Self::Network(error.to_string())
        }
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidBaseUrl(_) | Self::UnsupportedResponse(_) => "incompatible_api_response",
            Self::Network(_) => "network_unavailable",
            Self::Timeout(_) => "timeout",
            Self::Http { status: 404, .. } => "package_not_found",
            Self::Http { .. } => "http_status",
            Self::MalformedResponse(_) => "malformed_response",
        }
    }
}
impl fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBaseUrl(message)
            | Self::Network(message)
            | Self::Timeout(message)
            | Self::MalformedResponse(message)
            | Self::UnsupportedResponse(message) => f.write_str(message),
            Self::Http { status, server } => write!(
                f,
                "catalogue server returned HTTP {status}{}",
                server
                    .as_ref()
                    .map(|message| format!(": {message}"))
                    .unwrap_or_default()
            ),
        }
    }
}
impl std::error::Error for CatalogueError {}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;
    use serde_json::json;

    const PACKAGE: &str = r#"{"catalogueId":"catalogue.alpha","publisherId":"publisher-1","publisherType":"guild","publisherDisplayName":"Publisher","packageType":"dataset","nativeRpeId":"dataset.beta","datasetType":"campaign","datasetGroup":"world","name":"Dataset","shortDescription":"short","fullDescription":"full","currentRevision":7,"publicationState":"published","protected":true,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z"}"#;
    const REVISION: &str = r#"{"catalogueId":"catalogue.alpha","revision":7,"sha256":"abc","payloadByteSize":99,"changelog":"notes","publishedAt":"2026-01-02T00:00:00Z","minimumRpeVersion":"2.0","maximumRpeVersion":null,"requiredManagerProtocolVersion":"1"}"#;
    const DEPENDENCY: &str = r#"{"catalogueId":"catalogue.dependency","packageType":"ruleset","nativeRpeId":"ruleset.gamma","name":"Rules","minimumRevision":3}"#;

    fn server(status: u16, body: String, delay_ms: u64) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request);
            if delay_ms > 0 {
                thread::sleep(Duration::from_millis(delay_ms));
            }
            let reason = if status >= 400 { "Error" } else { "OK" };
            let response = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{address}/api/rpe/v1")
    }

    #[test]
    fn parses_catalogue_dataset_protection_dependencies_and_distinct_identity() {
        let mut package: serde_json::Value = serde_json::from_str(PACKAGE).unwrap();
        package["compatibility"] = json!({ "minimumRpeVersion": "2.0", "maximumRpeVersion": null, "requiredManagerProtocolVersion": "1" });
        package["dependencies"] = serde_json::from_str::<serde_json::Value>(DEPENDENCY)
            .map(|dependency| json!([dependency]))
            .unwrap();
        let body = json!({ "packages": [package], "pagination": { "limit": 1, "offset": 4, "hasMore": true } }).to_string();
        let client = CatalogueClient::new(&server(200, body, 0)).unwrap();
        let response = client.get_catalogue_packages(Some(1), Some(4)).unwrap();
        let package = &response.packages[0];
        assert_eq!(package.catalogue_id, "catalogue.alpha");
        assert_eq!(package.native_rpe_id, "dataset.beta");
        assert_ne!(package.catalogue_id, package.native_rpe_id);
        assert_eq!(package.current_revision, 7);
        assert!(package.protected);
        assert_eq!(package.dataset_group.as_deref(), Some("world"));
        assert_eq!(
            package.dependencies[0].package_type.as_deref(),
            Some("ruleset")
        );
        assert!(response.pagination.has_more);
    }

    #[test]
    fn parses_package_detail_revisions_and_publishers() {
        let detail = format!(
            r#"{{"package":{PACKAGE},"currentRevision":{REVISION},"revisionHistory":[{REVISION}],"dependencies":[{DEPENDENCY}]}}"#
        );
        let client = CatalogueClient::new(&server(200, detail, 0)).unwrap();
        let detail = client.get_package("catalogue.alpha").unwrap();
        assert_eq!(detail.current_revision.payload_byte_size, 99);
        assert_eq!(detail.revision_history[0].sha256, "abc");

        let publishers = r#"{"publishers":[{"publisherId":"publisher-1","publisherType":"guild","displayName":"Publisher","description":null,"iconUrl":null,"imageUrl":null,"visibility":"public","status":"active","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"}]}"#;
        let client = CatalogueClient::new(&server(200, publishers.into(), 0)).unwrap();
        assert_eq!(
            client.get_publishers().unwrap().publishers[0].display_name,
            "Publisher"
        );
    }

    #[test]
    fn parses_ruleset_revision_page() {
        let ruleset = PACKAGE
            .replace("\"packageType\":\"dataset\"", "\"packageType\":\"ruleset\"")
            .replace(
                "\"nativeRpeId\":\"dataset.beta\"",
                "\"nativeRpeId\":\"ruleset.gamma\"",
            );
        let body = format!(
            r#"{{"package":{ruleset},"currentRevision":7,"revisions":[{{"revision":{REVISION},"dependencies":[{DEPENDENCY}]}}],"pagination":{{"limit":100,"offset":0,"hasMore":false}}}}"#
        );
        let client = CatalogueClient::new(&server(200, body, 0)).unwrap();
        let response = client.get_revisions("catalogue.alpha", None, None).unwrap();
        assert_eq!(response.package.package_type, "ruleset");
        assert_eq!(
            response.revisions[0].dependencies[0].catalogue_id,
            "catalogue.dependency"
        );
    }

    #[test]
    fn maps_bad_json_http_network_and_timeout_errors() {
        let client = CatalogueClient::new(&server(200, "not json".into(), 0)).unwrap();
        assert!(matches!(
            client.get_publishers(),
            Err(CatalogueError::MalformedResponse(_))
        ));
        let client = CatalogueClient::new(&server(
            404,
            r#"{"error":{"code":"missing","message":"No package"}}"#.into(),
            0,
        ))
        .unwrap();
        assert!(matches!(
            client.get_package("missing"),
            Err(CatalogueError::Http { status: 404, .. })
        ));
        let client =
            CatalogueClient::new(&server(500, r#"{"error":{"code":"broken"}}"#.into(), 0)).unwrap();
        assert!(matches!(
            client.get_publishers(),
            Err(CatalogueError::Http { status: 500, .. })
        ));
        let client =
            CatalogueClient::with_timeout(&server(200, "{}".into(), 100), Duration::from_millis(5))
                .unwrap();
        assert!(matches!(
            client.get_publishers(),
            Err(CatalogueError::Timeout(_))
        ));
        let client = CatalogueClient::new("http://127.0.0.1:1/api/rpe/v1").unwrap();
        assert!(matches!(
            client.get_publishers(),
            Err(CatalogueError::Network(_))
        ));
    }
}
