use super::client::GcpClient;
use crate::resource::registry::ResourceDef;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, info};

/// List resources using the resource definition
/// parent_item: Optional parent item for sub-resources (provides context like {secret}, {cluster}, etc.)
pub async fn list_resources(
    client: &GcpClient,
    resource: &ResourceDef,
    parent_item: Option<&Value>,
) -> Result<Vec<Value>> {
    // Build extra context from parent item if available
    let extra = parent_item.map(|item| {
        let mut map = HashMap::new();
        // Extract common fields from parent that might be needed in URL
        // The full resource name (e.g., "projects/xxx/secrets/mysecret")
        if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
            map.insert("secret".to_string(), name.to_string());
            map.insert("parent".to_string(), name.to_string());
            map.insert("cluster".to_string(), name.to_string());
            map.insert("instance".to_string(), name.to_string());
            map.insert("topic".to_string(), name.to_string());
            map.insert("service".to_string(), name.to_string());
        }
        // Extract location if present
        if let Some(location) = item.get("location").and_then(|v| v.as_str()) {
            map.insert("location".to_string(), location.to_string());
        }
        map
    });

    let url = interpolate_url(
        &resource.api.base,
        &resource.api.path,
        client,
        extra.as_ref(),
    );

    debug!("Listing resources: {} -> {}", resource.display_name, url);

    let response = client.request(&resource.api.method, &url).await?;

    let items = if resource.response_path.is_empty() {
        if response.is_array() {
            response.as_array().unwrap().clone()
        } else {
            vec![response]
        }
    } else if resource.response_path.contains(".*") {
        // Handle aggregated list responses (e.g., "items.*.subnetworks")
        // Format: "items.*.fieldName" where items is a map of region -> { fieldName: [...] }
        extract_aggregated_items(&response, &resource.response_path)
    } else {
        response
            .pointer(&format!("/{}", resource.response_path.replace('.', "/")))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };

    info!("Listed {} {} items", items.len(), resource.display_name);
    Ok(items)
}

/// Extract items from aggregated list response
/// Path format: "items.*.subnetworks" means response.items is a map,
/// iterate all values and collect their "subnetworks" arrays
fn extract_aggregated_items(response: &Value, path: &str) -> Vec<Value> {
    let parts: Vec<&str> = path.split(".*.").collect();
    if parts.len() != 2 {
        return Vec::new();
    }

    let map_field = parts[0]; // e.g., "items"
    let array_field = parts[1]; // e.g., "subnetworks"

    let Some(map) = response.get(map_field).and_then(|v| v.as_object()) else {
        return Vec::new();
    };

    let mut all_items = Vec::new();

    for (_region_key, region_data) in map {
        if let Some(items) = region_data.get(array_field).and_then(|v| v.as_array()) {
            all_items.extend(items.clone());
        }
    }

    all_items
}

/// Execute an action on a resource
pub async fn execute_action(
    client: &GcpClient,
    resource: &ResourceDef,
    action_index: usize,
    item: &Value,
) -> Result<Value> {
    let action = resource
        .actions
        .get(action_index)
        .ok_or_else(|| anyhow::anyhow!("Action index {} out of bounds", action_index))?;

    info!(
        "Executing action '{}' on {}",
        action.display_name, resource.display_name
    );

    // Build extra placeholders from the item
    let mut extra = HashMap::new();

    // Add name from the item
    if let Some(name) = item.get(&resource.name_field).and_then(|v| v.as_str()) {
        extra.insert("name".to_string(), name.to_string());
        debug!("Action target name: {}", name);
    }

    // Add id from the item
    if let Some(id) = item.get(&resource.id_field).and_then(|v| v.as_str()) {
        extra.insert("id".to_string(), id.to_string());
    }

    // Extract zone from item if present (for zonal resources)
    if let Some(zone_url) = item.get("zone").and_then(|v| v.as_str()) {
        // Zone URLs look like: "https://www.googleapis.com/compute/v1/projects/PROJECT/zones/ZONE"
        if let Some(zone) = zone_url.split('/').next_back() {
            extra.insert("zone".to_string(), zone.to_string());
        }
    }

    // Extract region from item if present (for regional resources)
    if let Some(region_url) = item.get("region").and_then(|v| v.as_str()) {
        if let Some(region) = region_url.split('/').next_back() {
            extra.insert("region".to_string(), region.to_string());
        }
    }

    let url = interpolate_url(&resource.api.base, &action.api.path, client, Some(&extra));
    debug!("Action URL: {} {}", action.api.method, url);

    client.request(&action.api.method, &url).await
}

/// Interpolate URL placeholders with client context and extra values
fn interpolate_url(
    base: &str,
    path: &str,
    client: &GcpClient,
    extra: Option<&HashMap<String, String>>,
) -> String {
    let full_path = format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    );

    let mut result = full_path
        .replace("{project}", &client.project)
        .replace("{zone}", &client.zone)
        .replace("{region}", &derive_region_from_zone(&client.zone));

    // Apply extra placeholders if provided
    if let Some(extra) = extra {
        for (key, value) in extra {
            result = result.replace(&format!("{{{}}}", key), value);
        }
    }

    result
}

/// Derive region from zone (e.g., "us-central1-a" -> "us-central1")
fn derive_region_from_zone(zone: &str) -> String {
    // Zone format: region-zone (e.g., us-central1-a, europe-west1-b)
    // Region is everything except the last part after the last hyphen
    if let Some(idx) = zone.rfind('-') {
        zone[..idx].to_string()
    } else {
        zone.to_string()
    }
}
