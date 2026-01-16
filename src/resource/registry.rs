use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

const RESOURCE_FILES: &[&str] = &[
    include_str!("../resources/common.json"),
    include_str!("../resources/compute.json"),
    include_str!("../resources/storage.json"),
    include_str!("../resources/vpc.json"),
    include_str!("../resources/iam.json"),
    include_str!("../resources/gke.json"),
    include_str!("../resources/cloudsql.json"),
    include_str!("../resources/cloudrun.json"),
    include_str!("../resources/functions.json"),
    include_str!("../resources/pubsub.json"),
    include_str!("../resources/secretmanager.json"),
    include_str!("../resources/logging.json"),
    include_str!("../resources/bigquery.json"),
    include_str!("../resources/spanner.json"),
    include_str!("../resources/dns.json"),
    include_str!("../resources/loadbalancing.json"),
    include_str!("../resources/scheduler.json"),
    include_str!("../resources/tasks.json"),
    include_str!("../resources/artifactregistry.json"),
    include_str!("../resources/cloudbuild.json"),
    include_str!("../resources/dataproc.json"),
    include_str!("../resources/kms.json"),
    include_str!("../resources/memorystore.json"),
    include_str!("../resources/filestore.json"),
    include_str!("../resources/composer.json"),
    include_str!("../resources/dataflow.json"),
    include_str!("../resources/appengine.json"),
    include_str!("../resources/monitoring.json"),
    include_str!("../resources/endpoints.json"),
    include_str!("../resources/apigateway.json"),
    include_str!("../resources/servicedirectory.json"),
    include_str!("../resources/workflows.json"),
];

#[derive(Debug, Clone, Deserialize)]
pub struct ApiDef {
    pub base: String,
    pub path: String,
    pub method: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnDef {
    pub header: String,
    pub json_path: String,
    pub width: u16,
    #[serde(default)]
    pub color_map: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfirmConfig {
    pub message: String,
    #[serde(default)]
    pub destructive: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionApiDef {
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionDef {
    pub display_name: String,
    pub api: ActionApiDef,
    #[serde(default)]
    pub shortcut: Option<String>,
    #[serde(default)]
    pub confirm: Option<ConfirmConfig>,
}

/// Sub-resource definition from JSON
#[derive(Debug, Clone, Deserialize)]
pub struct SubResourceDef {
    pub resource_key: String,
    pub display_name: String,
    pub shortcut: String,
    #[allow(dead_code)]
    pub parent_id_field: String,
    #[allow(dead_code)]
    pub filter_param: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceDef {
    pub display_name: String,
    #[allow(dead_code)]
    pub service: String,
    pub api: ApiDef,
    pub response_path: String,
    pub id_field: String,
    pub name_field: String,
    pub columns: Vec<ColumnDef>,
    #[serde(default)]
    pub actions: Vec<ActionDef>,
    #[serde(default)]
    pub sub_resources: Vec<SubResourceDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColorDef {
    pub value: String,
    pub color: [u8; 3],
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResourceConfig {
    #[serde(default)]
    pub color_maps: HashMap<String, Vec<ColorDef>>,
    #[serde(default)]
    pub resources: HashMap<String, ResourceDef>,
}

static REGISTRY: OnceLock<ResourceConfig> = OnceLock::new();

pub fn get_registry() -> &'static ResourceConfig {
    REGISTRY.get_or_init(|| {
        let mut final_config = ResourceConfig {
            color_maps: HashMap::new(),
            resources: HashMap::new(),
        };

        for content in RESOURCE_FILES {
            let partial: ResourceConfig =
                serde_json::from_str(content).expect("Failed to parse embedded resource JSON");
            final_config.color_maps.extend(partial.color_maps);
            final_config.resources.extend(partial.resources);
        }

        final_config
    })
}

pub fn get_resource(key: &str) -> Option<&'static ResourceDef> {
    get_registry().resources.get(key)
}

pub fn get_all_resource_keys() -> Vec<&'static str> {
    get_registry()
        .resources
        .keys()
        .map(|s| s.as_str())
        .collect()
}

pub fn get_color_map(name: &str) -> Option<&'static Vec<ColorDef>> {
    get_registry().color_maps.get(name)
}

/// Get color for a value based on color map name
pub fn get_color_for_value(color_map_name: &str, value: &str) -> Option<[u8; 3]> {
    get_color_map(color_map_name)?
        .iter()
        .find(|c| c.value == value)
        .map(|c| c.color)
}

/// Extract a value from JSON using a path string
/// Supports dot notation (e.g., "networkInterfaces.0.networkIP")
/// and array notation (e.g., "networkInterfaces[0].networkIP")
pub fn extract_json_value(value: &Value, path: &str) -> String {
    // First try direct key access
    if let Some(v) = value.get(path) {
        return format_json_value(v);
    }

    // Convert path to JSON pointer format
    // "networkInterfaces[0].accessConfigs[0].natIP" -> "/networkInterfaces/0/accessConfigs/0/natIP"
    let ptr = format!("/{}", path.replace(['.', '['], "/").replace(']', ""));

    if let Some(v) = value.pointer(&ptr) {
        return format_json_value(v);
    }

    // Try handling nested Tags (GCP uses labels)
    if let Some(tag_key) = path.strip_prefix("labels.") {
        if let Some(labels) = value.get("labels") {
            if let Some(tag_value) = labels.get(tag_key) {
                return format_json_value(tag_value);
            }
        }
    }

    "-".to_string()
}

/// Format a JSON value as a string for display
fn format_json_value(v: &Value) -> String {
    match v {
        Value::String(s) => {
            // Clean up GCP URLs (e.g., machineType URLs)
            if s.starts_with("https://www.googleapis.com/") {
                // Extract the last part of the URL
                s.split('/').next_back().unwrap_or(s).to_string()
            } else if s.starts_with("projects/") {
                // Extract the last meaningful part
                s.split('/').next_back().unwrap_or(s).to_string()
            } else {
                s.clone()
            }
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "-".to_string(),
        Value::Array(arr) => {
            if arr.is_empty() {
                "-".to_string()
            } else {
                format!("[{} items]", arr.len())
            }
        }
        Value::Object(_) => "[object]".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_loads_successfully() {
        let registry = get_registry();
        assert!(
            !registry.resources.is_empty(),
            "Registry should have resources"
        );
        // We expect 60+ resources from all JSON files
        assert!(
            registry.resources.len() >= 50,
            "Registry should have at least 50 resources, found {}",
            registry.resources.len()
        );
    }

    #[test]
    fn test_vm_instances_resource_exists() {
        let resource = get_resource("vm-instances");
        assert!(resource.is_some(), "VM instances resource should exist");

        let resource = resource.unwrap();
        assert_eq!(resource.display_name, "VM Instances");
        assert_eq!(resource.service, "compute");
        assert!(resource.api.base.contains("compute.googleapis.com"));
        assert!(
            !resource.columns.is_empty(),
            "VM instances should have columns"
        );
    }

    #[test]
    fn test_gke_clusters_resource_exists() {
        let resource = get_resource("gke-clusters");
        assert!(resource.is_some(), "GKE clusters resource should exist");

        let resource = resource.unwrap();
        assert_eq!(resource.display_name, "GKE Clusters");
        assert_eq!(resource.service, "container");
        assert!(resource.api.base.contains("container.googleapis.com"));
    }

    #[test]
    fn test_storage_buckets_resource_exists() {
        let resource = get_resource("buckets");
        assert!(resource.is_some(), "Buckets resource should exist");

        let resource = resource.unwrap();
        assert_eq!(resource.display_name, "Storage Buckets");
        assert_eq!(resource.service, "storage");
    }

    #[test]
    fn test_iam_service_accounts_resource_exists() {
        let resource = get_resource("service-accounts");
        assert!(resource.is_some(), "Service accounts resource should exist");

        let resource = resource.unwrap();
        assert_eq!(resource.service, "iam");
        assert!(resource.api.base.contains("iam.googleapis.com"));
    }

    #[test]
    fn test_service_accounts_has_sub_resources() {
        let resource = get_resource("service-accounts").unwrap();
        assert!(
            !resource.sub_resources.is_empty(),
            "Service accounts should have sub-resources"
        );

        let keys_sub = resource
            .sub_resources
            .iter()
            .find(|s| s.resource_key == "sa-keys");
        assert!(
            keys_sub.is_some(),
            "Service accounts should have sa-keys sub-resource"
        );

        let keys_sub = keys_sub.unwrap();
        assert_eq!(keys_sub.shortcut, "k");
        assert!(!keys_sub.parent_id_field.is_empty());
    }

    #[test]
    fn test_secrets_has_sub_resources() {
        let resource = get_resource("secrets").unwrap();
        assert!(
            !resource.sub_resources.is_empty(),
            "Secrets should have sub-resources"
        );

        let versions_sub = resource
            .sub_resources
            .iter()
            .find(|s| s.resource_key == "secret-versions");
        assert!(
            versions_sub.is_some(),
            "Secrets should have secret-versions sub-resource"
        );
    }

    #[test]
    fn test_gke_clusters_has_node_pools_sub_resource() {
        let resource = get_resource("gke-clusters").unwrap();

        let node_pools_sub = resource
            .sub_resources
            .iter()
            .find(|s| s.resource_key == "node-pools");
        assert!(
            node_pools_sub.is_some(),
            "GKE clusters should have node-pools sub-resource"
        );
    }

    #[test]
    fn test_aggregated_list_resources() {
        // Resources that use aggregated list API should have wildcard response paths
        let subnets = get_resource("subnets");
        assert!(subnets.is_some(), "Subnets resource should exist");

        let subnets = subnets.unwrap();
        // Aggregated resources use items.*.subnetworks pattern
        assert!(
            subnets.response_path.contains("*") || subnets.api.path.contains("aggregated"),
            "Subnets should use aggregated list API"
        );
    }

    #[test]
    fn test_all_resources_have_required_fields() {
        let registry = get_registry();

        for (key, resource) in &registry.resources {
            assert!(
                !resource.display_name.is_empty(),
                "Resource '{}' should have a display_name",
                key
            );
            assert!(
                !resource.service.is_empty(),
                "Resource '{}' should have a service",
                key
            );
            assert!(
                !resource.api.base.is_empty(),
                "Resource '{}' should have an api.base",
                key
            );
            assert!(
                !resource.api.path.is_empty(),
                "Resource '{}' should have an api.path",
                key
            );
            assert!(
                !resource.response_path.is_empty(),
                "Resource '{}' should have a response_path",
                key
            );
            assert!(
                !resource.columns.is_empty(),
                "Resource '{}' should have at least one column",
                key
            );
        }
    }

    #[test]
    fn test_all_columns_have_required_fields() {
        let registry = get_registry();

        for (key, resource) in &registry.resources {
            for (i, col) in resource.columns.iter().enumerate() {
                assert!(
                    !col.header.is_empty(),
                    "Resource '{}' column {} should have a header",
                    key,
                    i
                );
                assert!(
                    !col.json_path.is_empty(),
                    "Resource '{}' column {} should have a json_path",
                    key,
                    i
                );
                assert!(
                    col.width > 0,
                    "Resource '{}' column {} should have width > 0",
                    key,
                    i
                );
            }
        }
    }

    #[test]
    fn test_actions_have_required_fields() {
        let registry = get_registry();

        for (key, resource) in &registry.resources {
            for action in &resource.actions {
                assert!(
                    !action.display_name.is_empty(),
                    "Resource '{}' action should have a display_name",
                    key
                );
                assert!(
                    !action.api.method.is_empty(),
                    "Resource '{}' action '{}' should have an api.method",
                    key,
                    action.display_name
                );
                assert!(
                    !action.api.path.is_empty(),
                    "Resource '{}' action '{}' should have an api.path",
                    key,
                    action.display_name
                );
            }
        }
    }

    #[test]
    fn test_sub_resources_have_required_fields() {
        let registry = get_registry();

        for (key, resource) in &registry.resources {
            for sub in &resource.sub_resources {
                assert!(
                    !sub.resource_key.is_empty(),
                    "Resource '{}' sub-resource should have a resource_key",
                    key
                );
                assert!(
                    !sub.display_name.is_empty(),
                    "Resource '{}' sub-resource should have a display_name",
                    key
                );
                assert!(
                    !sub.shortcut.is_empty(),
                    "Resource '{}' sub-resource should have a shortcut",
                    key
                );
                assert!(
                    !sub.parent_id_field.is_empty(),
                    "Resource '{}' sub-resource should have a parent_id_field",
                    key
                );
                // Verify the sub-resource target exists
                assert!(
                    get_resource(&sub.resource_key).is_some(),
                    "Resource '{}' sub-resource '{}' points to non-existent resource",
                    key,
                    sub.resource_key
                );
            }
        }
    }

    #[test]
    fn test_destructive_actions_have_confirmation() {
        let registry = get_registry();

        for (key, resource) in &registry.resources {
            for action in &resource.actions {
                // DELETE actions should have confirmation
                if action.api.method == "DELETE" {
                    assert!(
                        action.confirm.is_some(),
                        "Resource '{}' DELETE action '{}' should have confirmation",
                        key,
                        action.display_name
                    );
                    if let Some(confirm) = &action.confirm {
                        assert!(
                            confirm.destructive,
                            "Resource '{}' DELETE action '{}' should be marked as destructive",
                            key, action.display_name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_color_maps_exist() {
        let registry = get_registry();

        // Check that status color map exists (used by multiple resources)
        assert!(
            registry.color_maps.contains_key("status"),
            "Registry should have 'status' color map"
        );

        let status_map = registry.color_maps.get("status").unwrap();
        assert!(
            !status_map.is_empty(),
            "Status color map should have entries"
        );

        // Verify RUNNING status exists with green-ish color
        let running = status_map.iter().find(|c| c.value == "RUNNING");
        assert!(running.is_some(), "Status map should have RUNNING");
    }

    #[test]
    fn test_get_all_resource_keys() {
        let keys = get_all_resource_keys();
        assert!(!keys.is_empty(), "Should return resource keys");
        assert!(
            keys.contains(&"vm-instances"),
            "Should contain vm-instances"
        );
        assert!(keys.contains(&"buckets"), "Should contain buckets");
        assert!(
            keys.contains(&"gke-clusters"),
            "Should contain gke-clusters"
        );
    }

    #[test]
    fn test_extract_json_value_simple() {
        let json: Value = serde_json::json!({
            "name": "test-instance",
            "status": "RUNNING",
            "id": "12345"
        });

        assert_eq!(extract_json_value(&json, "name"), "test-instance");
        assert_eq!(extract_json_value(&json, "status"), "RUNNING");
        assert_eq!(extract_json_value(&json, "id"), "12345");
        assert_eq!(extract_json_value(&json, "nonexistent"), "-");
    }

    #[test]
    fn test_extract_json_value_nested() {
        let json: Value = serde_json::json!({
            "networkInterfaces": [
                {
                    "networkIP": "10.0.0.1",
                    "accessConfigs": [
                        {"natIP": "35.1.2.3"}
                    ]
                }
            ]
        });

        assert_eq!(
            extract_json_value(&json, "networkInterfaces[0].networkIP"),
            "10.0.0.1"
        );
        assert_eq!(
            extract_json_value(&json, "networkInterfaces[0].accessConfigs[0].natIP"),
            "35.1.2.3"
        );
    }

    #[test]
    fn test_extract_json_value_labels() {
        let json: Value = serde_json::json!({
            "name": "test",
            "labels": {
                "env": "production",
                "team": "platform"
            }
        });

        assert_eq!(extract_json_value(&json, "labels.env"), "production");
        assert_eq!(extract_json_value(&json, "labels.team"), "platform");
        assert_eq!(extract_json_value(&json, "labels.missing"), "-");
    }

    #[test]
    fn test_format_gcp_urls() {
        let json: Value = serde_json::json!({
            "machineType": "https://www.googleapis.com/compute/v1/projects/my-project/zones/us-central1-a/machineTypes/e2-medium",
            "zone": "projects/my-project/zones/us-central1-a"
        });

        // Should extract just the last part
        assert_eq!(extract_json_value(&json, "machineType"), "e2-medium");
        assert_eq!(extract_json_value(&json, "zone"), "us-central1-a");
    }

    #[test]
    fn test_get_color_for_value() {
        // This assumes the status color map exists
        let running_color = get_color_for_value("status", "RUNNING");
        assert!(
            running_color.is_some(),
            "Should find color for RUNNING status"
        );

        let unknown_color = get_color_for_value("status", "UNKNOWN_STATUS_XYZ");
        assert!(
            unknown_color.is_none(),
            "Should not find color for unknown status"
        );

        let invalid_map = get_color_for_value("nonexistent_map", "value");
        assert!(
            invalid_map.is_none(),
            "Should not find color in nonexistent map"
        );
    }

    // Test specific service resources exist
    #[test]
    fn test_compute_resources_exist() {
        assert!(get_resource("vm-instances").is_some());
        assert!(get_resource("disks").is_some());
        assert!(get_resource("snapshots").is_some());
        assert!(get_resource("images").is_some());
    }

    #[test]
    fn test_networking_resources_exist() {
        assert!(get_resource("networks").is_some());
        assert!(get_resource("subnets").is_some());
        assert!(get_resource("firewalls").is_some());
        assert!(get_resource("routes").is_some());
    }

    #[test]
    fn test_database_resources_exist() {
        assert!(get_resource("sql-instances").is_some());
        assert!(get_resource("spanner-instances").is_some());
        assert!(get_resource("bq-datasets").is_some());
    }

    #[test]
    fn test_serverless_resources_exist() {
        assert!(get_resource("functions").is_some());
        assert!(get_resource("cloudrun-services").is_some());
        assert!(get_resource("cloudrun-jobs").is_some());
    }

    #[test]
    fn test_messaging_resources_exist() {
        assert!(get_resource("pubsub-topics").is_some());
        assert!(get_resource("pubsub-subscriptions").is_some());
        assert!(get_resource("scheduler-jobs").is_some());
        assert!(get_resource("task-queues").is_some());
    }

    #[test]
    fn test_security_resources_exist() {
        assert!(get_resource("secrets").is_some());
        assert!(get_resource("kms-keyrings").is_some());
        assert!(get_resource("kms-keys").is_some());
    }

    #[test]
    fn test_monitoring_resources_exist() {
        assert!(get_resource("alert-policies").is_some());
        assert!(get_resource("uptime-checks").is_some());
        assert!(get_resource("log-sinks").is_some());
    }
}
