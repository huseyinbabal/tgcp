use anyhow::{anyhow, Context, Result};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::{debug, info, trace, warn};

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
const METADATA_PROJECT_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/project/project-id";

/// Cached token with expiry
#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    expires_at: chrono::DateTime<Utc>,
}

/// Global token cache
static TOKEN_CACHE: RwLock<Option<CachedToken>> = RwLock::new(None);

/// Service account credentials from JSON file
#[derive(Debug, Deserialize)]
struct ServiceAccountCredentials {
    #[serde(rename = "type")]
    cred_type: Option<String>,
    client_email: Option<String>,
    private_key: Option<String>,
    project_id: Option<String>,
    token_uri: Option<String>,
}

/// User credentials from ADC (application default credentials)
#[derive(Debug, Deserialize)]
struct UserCredentials {
    #[serde(rename = "type")]
    cred_type: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    refresh_token: Option<String>,
}

/// JWT claims for service account auth
#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    aud: String,
    iat: i64,
    exp: i64,
    scope: String,
}

/// Token response from Google OAuth
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<i64>,
    token_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TokenProvider;

impl TokenProvider {
    /// Get an access token using the following priority:
    /// 1. GCP_ACCESS_TOKEN env var (direct token)
    /// 2. GOOGLE_CREDENTIALS env var (inline JSON)
    /// 3. GOOGLE_APPLICATION_CREDENTIALS env var (path to JSON file)
    /// 4. Application Default Credentials (~/.config/gcloud/application_default_credentials.json)
    /// 5. GCP metadata server (for running on GCP infrastructure)
    pub async fn get_token() -> Result<String> {
        debug!("Getting access token");
        
        // Check cache first
        if let Some(token) = Self::get_cached_token() {
            trace!("Using cached token");
            return Ok(token);
        }

        // 1. Direct token from env
        if let Ok(token) = env::var("GCP_ACCESS_TOKEN") {
            info!("Using token from GCP_ACCESS_TOKEN env var");
            return Ok(token);
        }

        // 2. Inline JSON credentials
        if let Ok(json) = env::var("GOOGLE_CREDENTIALS") {
            info!("Using credentials from GOOGLE_CREDENTIALS env var");
            return Self::get_token_from_json(&json).await;
        }

        // 3. JSON file path
        if let Ok(path) = env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            info!("Using credentials from GOOGLE_APPLICATION_CREDENTIALS: {}", path);
            let json = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read credentials file: {}", path))?;
            return Self::get_token_from_json(&json).await;
        }

        // 4. Application Default Credentials (multiple possible locations)
        for adc_path in Self::get_adc_paths() {
            debug!("Checking ADC path: {:?}", adc_path);
            if adc_path.exists() {
                info!("Using Application Default Credentials from: {:?}", adc_path);
                let json = fs::read_to_string(&adc_path)
                    .with_context(|| format!("Failed to read ADC file: {:?}", adc_path))?;
                return Self::get_token_from_json(&json).await;
            }
        }

        // 5. Metadata server (GCP environment)
        debug!("Trying GCP metadata server");
        if let Ok(token) = Self::get_token_from_metadata().await {
            info!("Using token from GCP metadata server");
            return Ok(token);
        }

        warn!("No valid GCP credentials found");
        Err(anyhow!(
            "No valid GCP credentials found. Please either:\n\
             - Run 'gcloud auth application-default login'\n\
             - Set GOOGLE_APPLICATION_CREDENTIALS to a service account JSON file\n\
             - Set GCP_ACCESS_TOKEN environment variable"
        ))
    }

    /// Get project ID from credentials or environment
    pub async fn get_project() -> Result<String> {
        debug!("Getting GCP project");
        
        // 1. Direct env var
        if let Ok(p) = env::var("GCP_PROJECT") {
            info!("Using project from GCP_PROJECT: {}", p);
            return Ok(p);
        }
        if let Ok(p) = env::var("GOOGLE_CLOUD_PROJECT") {
            info!("Using project from GOOGLE_CLOUD_PROJECT: {}", p);
            return Ok(p);
        }
        if let Ok(p) = env::var("GCLOUD_PROJECT") {
            info!("Using project from GCLOUD_PROJECT: {}", p);
            return Ok(p);
        }

        // 2. From credentials file
        if let Ok(json) = env::var("GOOGLE_CREDENTIALS") {
            if let Ok(creds) = serde_json::from_str::<ServiceAccountCredentials>(&json) {
                if let Some(project) = creds.project_id {
                    info!("Using project from GOOGLE_CREDENTIALS: {}", project);
                    return Ok(project);
                }
            }
        }

        if let Ok(path) = env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            if let Ok(json) = fs::read_to_string(&path) {
                if let Ok(creds) = serde_json::from_str::<ServiceAccountCredentials>(&json) {
                    if let Some(project) = creds.project_id {
                        info!("Using project from GOOGLE_APPLICATION_CREDENTIALS: {}", project);
                        return Ok(project);
                    }
                }
            }
        }

        // 3. From ADC files
        for adc_path in Self::get_adc_paths() {
            debug!("Checking ADC for project: {:?}", adc_path);
            if let Ok(json) = fs::read_to_string(&adc_path) {
                if let Ok(creds) = serde_json::from_str::<ServiceAccountCredentials>(&json) {
                    if let Some(project) = creds.project_id {
                        info!("Using project from ADC: {}", project);
                        return Ok(project);
                    }
                }
                // ADC might have quota_project_id instead
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
                    if let Some(project) = value.get("quota_project_id").and_then(|v| v.as_str()) {
                        info!("Using quota_project_id from ADC: {}", project);
                        return Ok(project.to_string());
                    }
                }
            }
        }

        // 4. From metadata server
        debug!("Trying to get project from metadata server");
        if let Ok(project) = Self::get_project_from_metadata().await {
            info!("Using project from metadata server: {}", project);
            return Ok(project);
        }

        warn!("No GCP project found");
        Err(anyhow!(
            "No GCP project found. Please either:\n\
             - Set GCP_PROJECT or GOOGLE_CLOUD_PROJECT environment variable\n\
             - Run 'gcloud auth application-default login --project YOUR_PROJECT'"
        ))
    }

    /// Get token from JSON credentials (service account or user)
    async fn get_token_from_json(json: &str) -> Result<String> {
        let value: serde_json::Value =
            serde_json::from_str(json).context("Failed to parse credentials JSON")?;

        let cred_type = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("service_account");

        match cred_type {
            "service_account" => {
                let creds: ServiceAccountCredentials =
                    serde_json::from_str(json).context("Failed to parse service account JSON")?;
                Self::get_token_from_service_account(creds).await
            }
            "authorized_user" => {
                let creds: UserCredentials =
                    serde_json::from_str(json).context("Failed to parse user credentials JSON")?;
                Self::get_token_from_user_credentials(creds).await
            }
            _ => Err(anyhow!("Unknown credential type: {}", cred_type)),
        }
    }

    /// Get token using service account credentials (JWT -> OAuth2)
    async fn get_token_from_service_account(creds: ServiceAccountCredentials) -> Result<String> {
        let client_email = creds
            .client_email
            .ok_or_else(|| anyhow!("Missing client_email in service account"))?;
        let private_key = creds
            .private_key
            .ok_or_else(|| anyhow!("Missing private_key in service account"))?;
        let token_uri = creds.token_uri.unwrap_or_else(|| TOKEN_URI.to_string());

        let now = Utc::now();
        let exp = now + Duration::hours(1);

        let claims = JwtClaims {
            iss: client_email.clone(),
            sub: client_email,
            aud: token_uri.clone(),
            iat: now.timestamp(),
            exp: exp.timestamp(),
            scope: "https://www.googleapis.com/auth/cloud-platform".to_string(),
        };

        let header = Header::new(Algorithm::RS256);
        let key = EncodingKey::from_rsa_pem(private_key.as_bytes())
            .context("Failed to parse private key")?;

        let jwt = encode(&header, &claims, &key).context("Failed to encode JWT")?;

        // Exchange JWT for access token
        let client = Client::new();
        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ];

        let resp = client
            .post(&token_uri)
            .form(&params)
            .send()
            .await
            .context("Failed to request token")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Token exchange failed ({}): {}", status, text));
        }

        let token_resp: TokenResponse = resp.json().await.context("Failed to parse token response")?;

        // Cache the token
        let expires_at = Utc::now() + Duration::seconds(token_resp.expires_in.unwrap_or(3600) - 60);
        Self::cache_token(&token_resp.access_token, expires_at);

        Ok(token_resp.access_token)
    }

    /// Get token using user credentials (refresh token flow)
    async fn get_token_from_user_credentials(creds: UserCredentials) -> Result<String> {
        let client_id = creds
            .client_id
            .ok_or_else(|| anyhow!("Missing client_id in user credentials"))?;
        let client_secret = creds
            .client_secret
            .ok_or_else(|| anyhow!("Missing client_secret in user credentials"))?;
        let refresh_token = creds
            .refresh_token
            .ok_or_else(|| anyhow!("Missing refresh_token in user credentials"))?;

        let client = Client::new();
        let params = [
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        let resp = client
            .post(TOKEN_URI)
            .form(&params)
            .send()
            .await
            .context("Failed to refresh token")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Token refresh failed ({}): {}", status, text));
        }

        let token_resp: TokenResponse = resp.json().await.context("Failed to parse token response")?;

        // Cache the token
        let expires_at = Utc::now() + Duration::seconds(token_resp.expires_in.unwrap_or(3600) - 60);
        Self::cache_token(&token_resp.access_token, expires_at);

        Ok(token_resp.access_token)
    }

    /// Get token from GCP metadata server (for VMs, Cloud Run, GKE)
    async fn get_token_from_metadata() -> Result<String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()?;

        let resp = client
            .get(METADATA_TOKEN_URL)
            .header("Metadata-Flavor", "Google")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow!("Metadata server returned {}", resp.status()));
        }

        let token_resp: TokenResponse = resp.json().await?;

        // Cache the token
        let expires_at = Utc::now() + Duration::seconds(token_resp.expires_in.unwrap_or(3600) - 60);
        Self::cache_token(&token_resp.access_token, expires_at);

        Ok(token_resp.access_token)
    }

    /// Get project ID from metadata server
    async fn get_project_from_metadata() -> Result<String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()?;

        let resp = client
            .get(METADATA_PROJECT_URL)
            .header("Metadata-Flavor", "Google")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow!("Metadata server returned {}", resp.status()));
        }

        let project = resp.text().await?;
        
        // Validate that we got a real project ID, not HTML from a captive portal
        if project.contains('<') || project.contains('>') || project.to_lowercase().contains("html") {
            debug!("Metadata server returned HTML instead of project ID");
            return Err(anyhow!("Metadata server returned invalid response (possible captive portal)"));
        }

        Ok(project.trim().to_string())
    }

    /// Get all possible paths to Application Default Credentials file
    fn get_adc_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // 1. CLOUDSDK_CONFIG environment variable
        if let Ok(config_dir) = env::var("CLOUDSDK_CONFIG") {
            paths.push(PathBuf::from(config_dir).join("application_default_credentials.json"));
        }

        // 2. Home directory ~/.config/gcloud/ (Linux/macOS standard)
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".config/gcloud/application_default_credentials.json"));
        }

        // 3. XDG config dir (Linux)
        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("gcloud/application_default_credentials.json"));
        }

        // 4. Windows: %APPDATA%\gcloud\
        #[cfg(windows)]
        if let Ok(appdata) = env::var("APPDATA") {
            paths.push(PathBuf::from(appdata).join("gcloud\\application_default_credentials.json"));
        }

        paths
    }

    /// Get cached token if still valid
    fn get_cached_token() -> Option<String> {
        let cache = TOKEN_CACHE.read().ok()?;
        let cached = cache.as_ref()?;

        if cached.expires_at > Utc::now() {
            Some(cached.token.clone())
        } else {
            None
        }
    }

    /// Cache a token with expiry
    fn cache_token(token: &str, expires_at: chrono::DateTime<Utc>) {
        if let Ok(mut cache) = TOKEN_CACHE.write() {
            *cache = Some(CachedToken {
                token: token.to_string(),
                expires_at,
            });
        }
    }
}
