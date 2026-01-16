use super::auth::TokenProvider;
use anyhow::Result;
use reqwest::Client;
use tracing::{debug, error, info, trace};

#[derive(Clone)]
pub struct GcpClient {
    pub http: Client,
    pub project: String,
    pub zone: String,
    pub region: String,
}

impl GcpClient {
    pub async fn new(zone: Option<String>, project: Option<String>) -> Result<Self> {
        info!("Initializing GCP client");

        // Use provided project, or try to get from credentials
        let project = if let Some(p) = project {
            info!("Using project from config: {}", p);
            p
        } else {
            match TokenProvider::get_project().await {
                Ok(p) => {
                    info!("Using project from credentials: {}", p);
                    p
                }
                Err(e) => {
                    debug!("Could not get project from credentials: {}", e);
                    // Use empty string - will be set when user selects a project
                    String::new()
                }
            }
        };

        // Default zone
        let zone = zone.unwrap_or_else(|| "us-central1-a".to_string());

        // Derive region from zone
        let region = derive_region(&zone);

        info!(
            "GCP client initialized: project={}, zone={}, region={}",
            if project.is_empty() {
                "<none>"
            } else {
                &project
            },
            zone,
            region
        );

        Ok(Self {
            http: Client::new(),
            project,
            zone,
            region,
        })
    }

    /// List all projects accessible to the current user
    pub async fn list_projects(&self) -> Result<Vec<String>> {
        debug!("Listing GCP projects");

        let token = TokenProvider::get_token().await?;
        let url =
            "https://cloudresourcemanager.googleapis.com/v1/projects?filter=lifecycleState:ACTIVE";

        let res = self
            .http
            .get(url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;

        let status = res.status();
        if !status.is_success() {
            let text = res.text().await?;
            error!("Failed to list projects: {} - {}", status, text);
            return Err(anyhow::anyhow!("Failed to list projects: {}", status));
        }

        let json: serde_json::Value = res.json().await?;

        let projects: Vec<String> = json
            .get("projects")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| p.get("projectId").and_then(|id| id.as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        info!("Found {} projects", projects.len());
        debug!("Projects: {:?}", projects);

        Ok(projects)
    }

    /// Make an HTTP request to GCP API
    pub async fn request(&self, method: &str, url: &str) -> Result<serde_json::Value> {
        debug!("GCP API request: {} {}", method, url);

        let token = TokenProvider::get_token().await?;
        trace!("Got access token (length: {})", token.len());

        let req_method = match method.to_uppercase().as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "PATCH" => reqwest::Method::PATCH,
            "DELETE" => reqwest::Method::DELETE,
            _ => reqwest::Method::GET,
        };

        let res = self
            .http
            .request(req_method.clone(), url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .send()
            .await?;

        let status = res.status();
        debug!("GCP API response: {} {} -> {}", method, url, status);

        if !status.is_success() {
            let text = res.text().await?;
            error!(
                "GCP API Error {}: {} {}\nResponse: {}",
                status, method, url, text
            );
            return Err(anyhow::anyhow!("GCP API Error {}: {}", status, text));
        }

        // Handle empty responses (e.g., 204 No Content)
        let text = res.text().await?;
        if text.is_empty() {
            debug!("Empty response body, returning success");
            return Ok(serde_json::json!({"status": "success"}));
        }

        trace!("Response body length: {} bytes", text.len());
        let json: serde_json::Value = serde_json::from_str(&text)?;
        Ok(json)
    }

    /// Make a request with a JSON body
    #[allow(dead_code)]
    pub async fn request_with_body(
        &self,
        method: &str,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let token = TokenProvider::get_token().await?;

        let req_method = match method.to_uppercase().as_str() {
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "PATCH" => reqwest::Method::PATCH,
            _ => reqwest::Method::POST,
        };

        let res = self
            .http
            .request(req_method, url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await?;
            return Err(anyhow::anyhow!("GCP API Error {}: {}", status, text));
        }

        let text = res.text().await?;
        if text.is_empty() {
            return Ok(serde_json::json!({"status": "success"}));
        }

        let json: serde_json::Value = serde_json::from_str(&text)?;
        Ok(json)
    }

    /// Update zone and derive region
    pub fn set_zone(&mut self, zone: &str) {
        self.zone = zone.to_string();
        self.region = derive_region(zone);
    }
}

/// Derive region from zone (e.g., "us-central1-a" -> "us-central1")
fn derive_region(zone: &str) -> String {
    if let Some(idx) = zone.rfind('-') {
        zone[..idx].to_string()
    } else {
        zone.to_string()
    }
}
