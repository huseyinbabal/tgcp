//! Configuration management for tgcp
//!
//! Stores user preferences in ~/.config/tgcp/config.yaml (XDG compliant)
//! Falls back to ~/.tgcp/config.yaml if XDG dirs not available

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// User configuration stored on disk
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Last used GCP project ID
    #[serde(default)]
    pub project: Option<String>,

    /// Last used GCP zone
    #[serde(default)]
    pub zone: Option<String>,

    /// Last viewed resource type
    #[serde(default)]
    pub last_resource: Option<String>,
}

impl Config {
    /// Load config from disk, or return default if not found
    pub fn load() -> Self {
        let path = Self::config_path();

        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(contents) => match serde_yaml::from_str(&contents) {
                    Ok(config) => return config,
                    Err(e) => {
                        tracing::warn!("Failed to parse config: {}", e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read config: {}", e);
                }
            }
        }

        Self::default()
    }

    /// Save config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = serde_yaml::to_string(self)?;
        fs::write(&path, contents)?;

        tracing::debug!("Config saved to {:?}", path);
        Ok(())
    }

    /// Get the config file path
    /// Uses XDG config directory if available, otherwise ~/.tgcp/
    fn config_path() -> PathBuf {
        // Try XDG config dir first (e.g., ~/.config/tgcp/config.yaml)
        if let Some(config_dir) = dirs::config_dir() {
            return config_dir.join("tgcp").join("config.yaml");
        }

        // Fallback to home directory
        if let Some(home) = dirs::home_dir() {
            return home.join(".tgcp").join("config.yaml");
        }

        // Last resort: current directory
        PathBuf::from(".tgcp").join("config.yaml")
    }

    /// Update project and save
    pub fn set_project(&mut self, project: &str) -> Result<()> {
        self.project = Some(project.to_string());
        self.save()
    }

    /// Update zone and save
    pub fn set_zone(&mut self, zone: &str) -> Result<()> {
        self.zone = Some(zone.to_string());
        self.save()
    }

    /// Update last resource and save
    #[allow(dead_code)]
    pub fn set_last_resource(&mut self, resource: &str) -> Result<()> {
        self.last_resource = Some(resource.to_string());
        self.save()
    }

    /// Get effective project (env -> config -> None)
    pub fn effective_project(&self) -> Option<String> {
        // Priority: 1. Environment variable, 2. Config file
        std::env::var("GCP_PROJECT")
            .ok()
            .or_else(|| std::env::var("GOOGLE_CLOUD_PROJECT").ok())
            .or_else(|| std::env::var("GCLOUD_PROJECT").ok())
            .or_else(|| self.project.clone())
    }

    /// Get effective zone (env -> config -> default)
    pub fn effective_zone(&self) -> String {
        // Priority: 1. Environment variable, 2. Config file, 3. Default
        std::env::var("CLOUDSDK_COMPUTE_ZONE")
            .ok()
            .or_else(|| self.zone.clone())
            .unwrap_or_else(|| "us-central1-a".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.project.is_none());
        assert!(config.zone.is_none());
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = Config {
            project: Some("my-project".to_string()),
            zone: Some("us-central1-a".to_string()),
            last_resource: Some("vm-instances".to_string()),
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: Config = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(parsed.project, config.project);
        assert_eq!(parsed.zone, config.zone);
        assert_eq!(parsed.last_resource, config.last_resource);
    }
}
