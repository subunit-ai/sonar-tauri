use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{env, fmt, fs, path::PathBuf, time::Duration};

const BRIDGE_BASE_URL: &str = "http://127.0.0.1:7842";

#[derive(Clone)]
pub struct BridgeClient {
    http: reqwest::Client,
    base_url: String,
}

#[derive(Clone, Debug)]
pub struct BridgeHealth {
    pub version: Option<String>,
    pub paired: Option<bool>,
}

#[derive(Debug)]
pub enum BridgeClientError {
    EmptyToken(PathBuf),
    HomeDirUnavailable,
    HttpStatus {
        path: String,
        status: StatusCode,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Request(reqwest::Error),
}

impl fmt::Display for BridgeClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BridgeClientError::EmptyToken(path) => {
                write!(
                    formatter,
                    "bridge local API token is empty at {}",
                    path.display()
                )
            }
            BridgeClientError::HomeDirUnavailable => {
                write!(
                    formatter,
                    "HOME is unavailable and XDG_DATA_HOME is not set"
                )
            }
            BridgeClientError::HttpStatus { path, status } => {
                write!(formatter, "bridge request to {path} returned HTTP {status}")
            }
            BridgeClientError::Io { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            BridgeClientError::Request(error) => {
                write!(formatter, "bridge request failed: {error}")
            }
        }
    }
}

impl std::error::Error for BridgeClientError {}

impl From<reqwest::Error> for BridgeClientError {
    fn from(error: reqwest::Error) -> Self {
        BridgeClientError::Request(error)
    }
}

impl BridgeClient {
    pub fn new() -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|error| format!("failed to create bridge HTTP client: {error}"))?;

        Ok(Self {
            http,
            base_url: BRIDGE_BASE_URL.to_string(),
        })
    }

    pub async fn health(&self) -> Result<BridgeHealth, BridgeClientError> {
        let path = "/health";
        let response = self.http.get(self.endpoint_url(path)).send().await?;
        let status = response.status();
        if status != StatusCode::OK {
            return Err(BridgeClientError::HttpStatus {
                path: path.to_string(),
                status,
            });
        }

        let raw = response.json::<RawHealth>().await.ok();
        Ok(BridgeHealth {
            version: raw.as_ref().and_then(RawHealth::version),
            paired: raw.as_ref().and_then(RawHealth::paired),
        })
    }

    #[allow(dead_code)]
    pub async fn get_authed_json<T>(&self, path: &str) -> Result<T, BridgeClientError>
    where
        T: DeserializeOwned,
    {
        let token = read_local_api_token()?;
        let response = self
            .http
            .get(self.endpoint_url(path))
            .bearer_auth(token)
            .send()
            .await?;
        let status = response.status();

        if !status.is_success() {
            return Err(BridgeClientError::HttpStatus {
                path: path.to_string(),
                status,
            });
        }

        response
            .json::<T>()
            .await
            .map_err(BridgeClientError::Request)
    }

    pub async fn post_authed_json<B>(&self, path: &str, body: &B) -> Result<(), BridgeClientError>
    where
        B: Serialize + ?Sized,
    {
        let token = read_local_api_token()?;
        let response = self
            .http
            .post(self.endpoint_url(path))
            .bearer_auth(token)
            .json(body)
            .send()
            .await?;
        let status = response.status();

        if !status.is_success() {
            return Err(BridgeClientError::HttpStatus {
                path: path.to_string(),
                status,
            });
        }

        Ok(())
    }

    pub async fn post_help_request(&self) -> Result<(), BridgeClientError> {
        self.post_authed_json(
            "/forge/help-request",
            &serde_json::json!({ "kind": "forge.help_request" }),
        )
        .await
    }

    fn endpoint_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
}

#[allow(dead_code)]
pub fn local_api_token_path() -> Result<PathBuf, BridgeClientError> {
    let state_dir = match env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.as_os_str().is_empty() => PathBuf::from(value),
        _ => {
            let home = env::var_os("HOME").ok_or(BridgeClientError::HomeDirUnavailable)?;
            PathBuf::from(home).join(".local/share")
        }
    };

    Ok(state_dir.join("subunit-bridge/local-api-token"))
}

fn read_local_api_token() -> Result<String, BridgeClientError> {
    let path = local_api_token_path()?;
    let token = fs::read_to_string(&path)
        .map_err(|source| BridgeClientError::Io {
            path: path.clone(),
            source,
        })?
        .trim()
        .to_string();

    if token.is_empty() {
        return Err(BridgeClientError::EmptyToken(path));
    }

    Ok(token)
}

#[derive(Debug, Deserialize)]
struct RawHealth {
    version: Option<Value>,
    bridge_version: Option<Value>,
    #[serde(rename = "bridgeVersion")]
    bridge_version_camel: Option<Value>,
    paired: Option<Value>,
}

impl RawHealth {
    fn version(&self) -> Option<String> {
        self.version
            .as_ref()
            .or(self.bridge_version.as_ref())
            .or(self.bridge_version_camel.as_ref())
            .and_then(value_to_string)
    }

    fn paired(&self) -> Option<bool> {
        self.paired.as_ref().and_then(value_to_bool)
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.is_empty() => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(value) if value.eq_ignore_ascii_case("true") => Some(true),
        Value::String(value) if value.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}
