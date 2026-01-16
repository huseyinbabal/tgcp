use anyhow::Result;
use crossterm::event::KeyCode;
use serde_json::Value;

use crate::config::Config;
use crate::gcp::client::GcpClient;
use crate::gcp::dispatch::{execute_action, list_resources};
use crate::resource::registry::{
    extract_json_value, get_all_resource_keys, get_resource, ResourceDef,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,   // Viewing list
    Command,  // : command input
    Help,     // ? help popup
    Confirm,  // Confirmation dialog
    Warning,  // Warning/info dialog (OK only)
    Projects, // Project selection
    Zones,    // Zone selection
    Describe, // Viewing JSON details of selected item
}

/// Pending action that requires confirmation
#[derive(Debug, Clone)]
pub struct PendingAction {
    /// Display message for confirmation dialog
    pub message: String,
    /// If true, show as destructive (red)
    pub destructive: bool,
    /// Currently selected option (true = Yes, false = No)
    pub selected_yes: bool,
    /// Action data
    pub action_key: String,
    #[allow(dead_code)]
    pub resource_id: String,
}

/// Parent context for hierarchical navigation
#[derive(Debug, Clone)]
pub struct ParentContext {
    /// Parent resource key
    pub resource_key: String,
    /// Parent item (the selected item)
    pub item: Value,
    /// Display name for breadcrumb
    pub display_name: String,
}

pub struct App {
    // GCP Client
    pub client: GcpClient,

    // Current resource being viewed
    pub resource_key: String,

    // Dynamic data storage (JSON)
    pub items: Vec<Value>,
    pub filtered_items: Vec<Value>,

    // Navigation state
    pub selected: usize,
    pub mode: Mode,
    pub filter_text: String,
    pub filter_active: bool,

    // Hierarchical navigation
    pub parent_context: Option<ParentContext>,
    pub navigation_stack: Vec<ParentContext>,

    // Command input
    pub command_text: String,
    pub command_suggestions: Vec<String>,
    pub command_suggestion_selected: usize,
    pub command_preview: Option<String>, // Ghost text for hovered suggestion

    // Project/Zone
    pub project: String,
    pub zone: String,
    pub available_projects: Vec<String>,
    pub available_zones: Vec<String>,
    pub projects_selected: usize,
    pub zones_selected: usize,

    // Confirmation
    pub pending_action: Option<PendingAction>,

    // UI state
    pub loading: bool,
    pub error: Option<String>,
    pub describe_scroll: usize,
    pub describe_data: Option<Value>, // Full resource details from describe API

    // Auto-refresh
    pub last_refresh: std::time::Instant,

    // Key press tracking for sequences (e.g., 'gg')
    pub last_key_press: Option<(KeyCode, std::time::Instant)>,

    // Warning message for modal dialog
    pub warning_message: Option<String>,

    // Configuration (persisted to disk)
    pub config: Config,

    // Read-only mode (blocks all write operations)
    pub readonly: bool,
}

impl App {
    pub async fn new(
        zone: Option<String>,
        project: Option<String>,
        config: Config,
        readonly: bool,
    ) -> Result<Self> {
        let client = GcpClient::new(zone.clone(), project.clone()).await?;
        let project = client.project.clone();
        let zone = client.zone.clone();

        // Default available zones (can be fetched from API later)
        let available_zones = vec![
            "us-central1-a".to_string(),
            "us-central1-b".to_string(),
            "us-central1-c".to_string(),
            "us-east1-b".to_string(),
            "us-east1-c".to_string(),
            "us-west1-a".to_string(),
            "us-west1-b".to_string(),
            "europe-west1-b".to_string(),
            "europe-west1-c".to_string(),
            "asia-east1-a".to_string(),
            "asia-east1-b".to_string(),
            "asia-northeast1-a".to_string(),
        ];

        // Fetch available projects from GCP API
        let available_projects = match client.list_projects().await {
            Ok(projects) => {
                if projects.is_empty() {
                    vec![project.clone()]
                } else {
                    projects
                }
            }
            Err(_) => vec![project.clone()],
        };

        // If no project is set, start in Projects mode to let user select one
        let initial_mode = if project.is_empty() {
            Mode::Projects
        } else {
            Mode::Normal
        };

        Ok(Self {
            client,
            resource_key: "vm-instances".to_string(),
            items: Vec::new(),
            filtered_items: Vec::new(),
            selected: 0,
            mode: initial_mode,
            filter_text: String::new(),
            filter_active: false,
            parent_context: None,
            navigation_stack: Vec::new(),
            command_text: String::new(),
            command_suggestions: Vec::new(),
            command_suggestion_selected: 0,
            command_preview: None,
            project,
            zone,
            available_projects,
            available_zones,
            projects_selected: 0,
            zones_selected: 0,
            pending_action: None,
            loading: false,
            error: None,
            describe_scroll: 0,
            describe_data: None,
            last_refresh: std::time::Instant::now(),
            last_key_press: None,
            warning_message: None,
            config,
            readonly,
        })
    }

    /// Check if a project is selected
    pub fn has_project(&self) -> bool {
        !self.project.is_empty()
    }

    /// Create App from pre-initialized components (used with splash screen)
    #[allow(clippy::too_many_arguments)]
    #[allow(dead_code)]
    pub fn from_initialized(
        client: GcpClient,
        project: String,
        zone: String,
        available_projects: Vec<String>,
        available_zones: Vec<String>,
        initial_items: Vec<Value>,
        config: Config,
        readonly: bool,
    ) -> Self {
        let filtered_items = initial_items.clone();

        Self {
            client,
            resource_key: "vm-instances".to_string(),
            items: initial_items,
            filtered_items,
            selected: 0,
            mode: Mode::Normal,
            filter_text: String::new(),
            filter_active: false,
            parent_context: None,
            navigation_stack: Vec::new(),
            command_text: String::new(),
            command_suggestions: Vec::new(),
            command_suggestion_selected: 0,
            command_preview: None,
            project,
            zone,
            available_projects,
            available_zones,
            projects_selected: 0,
            zones_selected: 0,
            pending_action: None,
            loading: false,
            error: None,
            describe_scroll: 0,
            describe_data: None,
            last_refresh: std::time::Instant::now(),
            last_key_press: None,
            warning_message: None,
            config,
            readonly,
        }
    }

    /// Check if auto-refresh is needed (every 5 seconds)
    pub fn needs_refresh(&self) -> bool {
        // Only auto-refresh in Normal mode, not when in dialogs/command/etc.
        if self.mode != Mode::Normal {
            return false;
        }
        // Don't refresh while already loading
        if self.loading {
            return false;
        }
        // Don't auto-refresh if there's an error (user needs to dismiss it first)
        if self.error.is_some() {
            return false;
        }
        // Don't refresh if no project is selected
        if !self.has_project() {
            return false;
        }
        self.last_refresh.elapsed() >= std::time::Duration::from_secs(5)
    }

    /// Reset refresh timer
    pub fn mark_refreshed(&mut self) {
        self.last_refresh = std::time::Instant::now();
    }

    // =========================================================================
    // Resource Definition Access
    // =========================================================================

    /// Get current resource definition
    pub fn current_resource(&self) -> Option<&'static ResourceDef> {
        get_resource(&self.resource_key)
    }

    /// Get available commands for autocomplete
    pub fn get_available_commands(&self) -> Vec<String> {
        let mut commands: Vec<String> = get_all_resource_keys()
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Add projects and zones commands
        commands.push("projects".to_string());
        commands.push("zones".to_string());

        commands.sort();
        commands
    }

    // =========================================================================
    // Data Fetching
    // =========================================================================

    /// Fetch data for current resource
    pub async fn refresh(&mut self) {
        self.loading = true;
        self.error = None;

        if let Some(resource) = get_resource(&self.resource_key) {
            // Get parent item if we're in a sub-resource context
            let parent_item = self.parent_context.as_ref().map(|ctx| &ctx.item);

            match list_resources(&self.client, resource, parent_item).await {
                Ok(items) => {
                    let prev_selected = self.selected;
                    self.items = items;
                    self.apply_filter();

                    // Try to keep the same selection index
                    if prev_selected < self.filtered_items.len() {
                        self.selected = prev_selected;
                    } else {
                        self.selected = 0;
                    }
                }
                Err(e) => {
                    self.show_error(&e.to_string());
                    self.items.clear();
                    self.filtered_items.clear();
                    self.selected = 0;
                }
            }
        } else {
            self.show_error(&format!("Resource {} not found", self.resource_key));
        }

        self.loading = false;
        self.mark_refreshed();
    }

    // =========================================================================
    // Filtering
    // =========================================================================

    /// Apply text filter to items
    pub fn apply_filter(&mut self) {
        let filter = self.filter_text.to_lowercase();

        if filter.is_empty() {
            self.filtered_items = self.items.clone();
        } else {
            let resource = self.current_resource();
            self.filtered_items = self
                .items
                .iter()
                .filter(|item| {
                    // Search in name field and id field
                    if let Some(res) = resource {
                        let name = extract_json_value(item, &res.name_field).to_lowercase();
                        let id = extract_json_value(item, &res.id_field).to_lowercase();
                        name.contains(&filter) || id.contains(&filter)
                    } else {
                        // Fallback: search in JSON string
                        item.to_string().to_lowercase().contains(&filter)
                    }
                })
                .cloned()
                .collect();
        }

        // Adjust selection
        if self.selected >= self.filtered_items.len() && !self.filtered_items.is_empty() {
            self.selected = self.filtered_items.len() - 1;
        }
    }

    #[allow(dead_code)]
    pub fn toggle_filter(&mut self) {
        self.filter_active = !self.filter_active;
    }

    pub fn clear_filter(&mut self) {
        self.filter_text.clear();
        self.filter_active = false;
        self.apply_filter();
    }

    // =========================================================================
    // Navigation
    // =========================================================================

    pub fn selected_item(&self) -> Option<&Value> {
        self.filtered_items.get(self.selected)
    }

    pub fn selected_item_json(&self) -> Option<String> {
        // Use describe_data if available (full details), otherwise fall back to list data
        if let Some(ref data) = self.describe_data {
            return Some(serde_json::to_string_pretty(data).unwrap_or_default());
        }
        self.selected_item()
            .map(|item| serde_json::to_string_pretty(item).unwrap_or_default())
    }

    /// Get the number of lines in the describe content
    pub fn describe_line_count(&self) -> usize {
        self.selected_item_json()
            .map(|s| s.lines().count())
            .unwrap_or(0)
    }

    /// Scroll describe view to bottom
    pub fn describe_scroll_to_bottom(&mut self, visible_lines: usize) {
        let total = self.describe_line_count();
        self.describe_scroll = total.saturating_sub(visible_lines);
    }

    pub fn next(&mut self) {
        match self.mode {
            Mode::Projects => {
                if !self.available_projects.is_empty() {
                    self.projects_selected =
                        (self.projects_selected + 1).min(self.available_projects.len() - 1);
                }
            }
            Mode::Zones => {
                if !self.available_zones.is_empty() {
                    self.zones_selected =
                        (self.zones_selected + 1).min(self.available_zones.len() - 1);
                }
            }
            _ => {
                if !self.filtered_items.is_empty() {
                    self.selected = (self.selected + 1).min(self.filtered_items.len() - 1);
                }
            }
        }
    }

    pub fn previous(&mut self) {
        match self.mode {
            Mode::Projects => {
                self.projects_selected = self.projects_selected.saturating_sub(1);
            }
            Mode::Zones => {
                self.zones_selected = self.zones_selected.saturating_sub(1);
            }
            _ => {
                self.selected = self.selected.saturating_sub(1);
            }
        }
    }

    pub fn go_to_top(&mut self) {
        match self.mode {
            Mode::Projects => self.projects_selected = 0,
            Mode::Zones => self.zones_selected = 0,
            _ => self.selected = 0,
        }
    }

    pub fn go_to_bottom(&mut self) {
        match self.mode {
            Mode::Projects => {
                if !self.available_projects.is_empty() {
                    self.projects_selected = self.available_projects.len() - 1;
                }
            }
            Mode::Zones => {
                if !self.available_zones.is_empty() {
                    self.zones_selected = self.available_zones.len() - 1;
                }
            }
            _ => {
                if !self.filtered_items.is_empty() {
                    self.selected = self.filtered_items.len() - 1;
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn page_down(&mut self, page_size: usize) {
        match self.mode {
            Mode::Projects => {
                if !self.available_projects.is_empty() {
                    self.projects_selected =
                        (self.projects_selected + page_size).min(self.available_projects.len() - 1);
                }
            }
            Mode::Zones => {
                if !self.available_zones.is_empty() {
                    self.zones_selected =
                        (self.zones_selected + page_size).min(self.available_zones.len() - 1);
                }
            }
            _ => {
                if !self.filtered_items.is_empty() {
                    self.selected = (self.selected + page_size).min(self.filtered_items.len() - 1);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn page_up(&mut self, page_size: usize) {
        match self.mode {
            Mode::Projects => {
                self.projects_selected = self.projects_selected.saturating_sub(page_size);
            }
            Mode::Zones => {
                self.zones_selected = self.zones_selected.saturating_sub(page_size);
            }
            _ => {
                self.selected = self.selected.saturating_sub(page_size);
            }
        }
    }

    // =========================================================================
    // Mode Transitions
    // =========================================================================

    pub fn enter_command_mode(&mut self) {
        self.mode = Mode::Command;
        self.command_text.clear();
        self.command_suggestions = self.get_available_commands();
        self.command_suggestion_selected = 0;
        self.command_preview = None;
    }

    pub fn update_command_suggestions(&mut self) {
        let input = self.command_text.to_lowercase();
        let all_commands = self.get_available_commands();

        if input.is_empty() {
            self.command_suggestions = all_commands;
        } else {
            self.command_suggestions = all_commands
                .into_iter()
                .filter(|cmd| cmd.contains(&input))
                .collect();
        }

        if self.command_suggestion_selected >= self.command_suggestions.len() {
            self.command_suggestion_selected = 0;
        }

        // Update preview to show current selection
        self.update_preview();
    }

    fn update_preview(&mut self) {
        if self.command_suggestions.is_empty() {
            self.command_preview = None;
        } else {
            self.command_preview = self
                .command_suggestions
                .get(self.command_suggestion_selected)
                .cloned();
        }
    }

    pub fn next_suggestion(&mut self) {
        if !self.command_suggestions.is_empty() {
            self.command_suggestion_selected =
                (self.command_suggestion_selected + 1) % self.command_suggestions.len();
            self.update_preview();
        }
    }

    pub fn prev_suggestion(&mut self) {
        if !self.command_suggestions.is_empty() {
            if self.command_suggestion_selected == 0 {
                self.command_suggestion_selected = self.command_suggestions.len() - 1;
            } else {
                self.command_suggestion_selected -= 1;
            }
            self.update_preview();
        }
    }

    pub fn apply_suggestion(&mut self) {
        // Apply the preview to command_text (on Tab/Right)
        if let Some(preview) = &self.command_preview {
            self.command_text = preview.clone();
            self.update_command_suggestions();
        }
    }

    pub fn enter_help_mode(&mut self) {
        self.mode = Mode::Help;
    }

    pub fn enter_describe_mode(&mut self) {
        if self.filtered_items.is_empty() {
            return;
        }

        self.mode = Mode::Describe;
        self.describe_scroll = 0;
        self.describe_data = self.selected_item().cloned();
    }

    /// Enter confirmation mode for an action
    #[allow(dead_code)]
    pub fn enter_confirm_mode(&mut self, pending: PendingAction) {
        self.pending_action = Some(pending);
        self.mode = Mode::Confirm;
    }

    /// Show a warning modal with OK button
    pub fn show_warning(&mut self, message: &str) {
        self.warning_message = Some(message.to_string());
        self.error = None;
        self.mode = Mode::Warning;
    }

    /// Show an error modal with OK button
    pub fn show_error(&mut self, message: &str) {
        self.error = Some(message.to_string());
        self.warning_message = None;
        self.mode = Mode::Warning;
    }

    pub fn enter_projects_mode(&mut self) {
        self.projects_selected = self
            .available_projects
            .iter()
            .position(|p| p == &self.project)
            .unwrap_or(0);
        self.mode = Mode::Projects;
    }

    pub fn enter_zones_mode(&mut self) {
        self.zones_selected = self
            .available_zones
            .iter()
            .position(|z| z == &self.zone)
            .unwrap_or(0);
        self.mode = Mode::Zones;
    }

    pub fn exit_mode(&mut self) {
        self.mode = Mode::Normal;
        self.pending_action = None;
        self.describe_data = None;
        self.warning_message = None;
        self.error = None;
    }

    // =========================================================================
    // Resource Navigation
    // =========================================================================

    /// Navigate to a resource (top-level)
    pub async fn navigate_to_resource(&mut self, resource_key: &str) {
        if get_resource(resource_key).is_none() {
            self.error = Some(format!("Unknown resource: {}", resource_key));
            return;
        }

        // Clear parent context when navigating to top-level resource
        self.parent_context = None;
        self.navigation_stack.clear();
        self.resource_key = resource_key.to_string();
        self.selected = 0;
        self.filter_text.clear();
        self.filter_active = false;
        self.mode = Mode::Normal;

        self.refresh().await;
    }

    /// Navigate to sub-resource with parent context
    pub async fn navigate_to_sub_resource(&mut self, sub_resource_key: &str) {
        let Some(selected_item) = self.selected_item().cloned() else {
            return;
        };

        let Some(current_resource) = self.current_resource() else {
            return;
        };

        // Verify this is a valid sub-resource
        let is_valid = current_resource
            .sub_resources
            .iter()
            .any(|s| s.resource_key == sub_resource_key);

        if !is_valid {
            self.error = Some(format!(
                "{} is not a sub-resource of {}",
                sub_resource_key, self.resource_key
            ));
            return;
        }

        // Get display name for parent
        let display_name = extract_json_value(&selected_item, &current_resource.name_field);
        let id = extract_json_value(&selected_item, &current_resource.id_field);
        let display = if display_name != "-" {
            display_name
        } else {
            id
        };

        // Push current context to stack
        if let Some(ctx) = self.parent_context.take() {
            self.navigation_stack.push(ctx);
        }

        // Set new parent context
        self.parent_context = Some(ParentContext {
            resource_key: self.resource_key.clone(),
            item: selected_item,
            display_name: display,
        });

        // Navigate
        self.resource_key = sub_resource_key.to_string();
        self.selected = 0;
        self.filter_text.clear();
        self.filter_active = false;

        self.refresh().await;
    }

    /// Navigate back to parent resource
    pub async fn navigate_back(&mut self) {
        if let Some(parent) = self.parent_context.take() {
            // Pop from navigation stack if available
            self.parent_context = self.navigation_stack.pop();

            // Navigate to parent resource
            self.resource_key = parent.resource_key;
            self.selected = 0;
            self.filter_text.clear();
            self.filter_active = false;

            self.refresh().await;
        }
    }

    /// Get breadcrumb path
    pub fn get_breadcrumb(&self) -> Vec<String> {
        let mut path = Vec::new();

        for ctx in &self.navigation_stack {
            path.push(format!("{}:{}", ctx.resource_key, ctx.display_name));
        }

        if let Some(ctx) = &self.parent_context {
            path.push(format!("{}:{}", ctx.resource_key, ctx.display_name));
        }

        path.push(self.resource_key.clone());
        path
    }

    // =========================================================================
    // Zone/Project Switching
    // =========================================================================

    pub async fn switch_zone(&mut self, zone: &str) {
        self.zone = zone.to_string();
        self.client.set_zone(zone);
        // Save to config
        if let Err(e) = self.config.set_zone(zone) {
            tracing::warn!("Failed to save zone to config: {}", e);
        }
    }

    pub async fn switch_project(&mut self, project: &str) {
        self.project = project.to_string();
        self.client.project = project.to_string();
        // Save to config
        if let Err(e) = self.config.set_project(project) {
            tracing::warn!("Failed to save project to config: {}", e);
        }
    }

    pub async fn select_project(&mut self) {
        if let Some(project) = self.available_projects.get(self.projects_selected) {
            let project = project.clone();
            self.switch_project(&project).await;
            self.refresh().await;
        }
        self.exit_mode();
    }

    pub async fn select_zone(&mut self) {
        if let Some(zone) = self.available_zones.get(self.zones_selected) {
            let zone = zone.clone();
            self.switch_zone(&zone).await;
            self.refresh().await;
        }
        self.exit_mode();
    }

    // =========================================================================
    // Command Execution
    // =========================================================================

    pub async fn execute_command(&mut self) -> bool {
        // Use preview if user navigated to a suggestion, otherwise use typed text
        let command_text = if self.command_text.is_empty() {
            self.command_preview.clone().unwrap_or_default()
        } else if let Some(preview) = &self.command_preview {
            // If preview matches what would be completed, use preview
            if preview.contains(&self.command_text) {
                preview.clone()
            } else {
                self.command_text.clone()
            }
        } else {
            self.command_text.clone()
        };

        let parts: Vec<&str> = command_text.split_whitespace().collect();

        if parts.is_empty() {
            return false;
        }

        let cmd = parts[0];

        match cmd {
            "q" | "quit" => return true,
            "back" => {
                self.navigate_back().await;
            }
            "projects" => {
                self.enter_projects_mode();
                return false; // Don't reset mode
            }
            "zones" => {
                self.enter_zones_mode();
                return false; // Don't reset mode
            }
            "zone" if parts.len() > 1 => {
                self.switch_zone(parts[1]).await;
                self.refresh().await;
            }
            "project" if parts.len() > 1 => {
                self.switch_project(parts[1]).await;
                self.refresh().await;
            }
            _ => {
                // Check if it's a known resource
                if get_resource(cmd).is_some() {
                    // Check if it's a sub-resource of current
                    if let Some(resource) = self.current_resource() {
                        let is_sub = resource.sub_resources.iter().any(|s| s.resource_key == cmd);
                        if is_sub && self.selected_item().is_some() {
                            self.navigate_to_sub_resource(cmd).await;
                        } else {
                            self.navigate_to_resource(cmd).await;
                        }
                    } else {
                        self.navigate_to_resource(cmd).await;
                    }
                } else {
                    self.error = Some(format!("Unknown command: {}", cmd));
                }
            }
        }

        self.mode = Mode::Normal;
        false
    }

    // =========================================================================
    // Action Execution
    // =========================================================================

    /// Find action by shortcut key and return its index
    pub fn find_action_by_shortcut(&self, shortcut: &str) -> Option<usize> {
        self.current_resource()?
            .actions
            .iter()
            .position(|a| a.shortcut.as_deref() == Some(shortcut))
    }

    /// Find sub-resource by shortcut key and return its resource_key
    pub fn find_sub_resource_by_shortcut(&self, shortcut: &str) -> Option<String> {
        // Must have a selected item to navigate to sub-resource
        self.selected_item()?;

        self.current_resource()?
            .sub_resources
            .iter()
            .find(|s| s.shortcut == shortcut)
            .map(|s| s.resource_key.clone())
    }

    /// Get action hints for the current resource (for display in footer)
    #[allow(dead_code)]
    pub fn get_action_hints(&self) -> Vec<(String, String)> {
        let Some(resource) = self.current_resource() else {
            return Vec::new();
        };

        resource
            .actions
            .iter()
            .filter_map(|a| {
                a.shortcut
                    .as_ref()
                    .map(|s| (s.clone(), a.display_name.clone()))
            })
            .collect()
    }

    /// Trigger an action by index - either execute immediately or show confirmation
    pub fn trigger_action(&mut self, action_index: usize) {
        // Block actions in readonly mode
        if self.readonly {
            self.show_warning("This operation is not supported in read-only mode");
            return;
        }

        let Some(resource) = self.current_resource() else {
            return;
        };

        let Some(action) = resource.actions.get(action_index) else {
            return;
        };

        let Some(item) = self.selected_item() else {
            self.show_warning("No item selected");
            return;
        };

        // Get item name for display
        let item_name = extract_json_value(item, &resource.name_field);
        let item_id = extract_json_value(item, &resource.id_field);

        // Check if action requires confirmation
        if let Some(confirm) = &action.confirm {
            let message = confirm
                .message
                .replace("{name}", &item_name)
                .replace("{id}", &item_id);

            self.pending_action = Some(PendingAction {
                message,
                destructive: confirm.destructive,
                selected_yes: false, // Default to No for safety
                action_key: action_index.to_string(),
                resource_id: item_id,
            });
            self.mode = Mode::Confirm;
        } else {
            // Execute immediately
            self.pending_action = Some(PendingAction {
                message: String::new(),
                destructive: false,
                selected_yes: true,
                action_key: action_index.to_string(),
                resource_id: item_id,
            });
        }
    }

    /// Execute the pending action
    pub async fn execute_pending_action(&mut self) {
        let Some(pending) = self.pending_action.take() else {
            return;
        };

        if !pending.selected_yes {
            self.exit_mode();
            return;
        }

        let action_index: usize = match pending.action_key.parse() {
            Ok(idx) => idx,
            Err(_) => {
                self.show_warning("Invalid action index");
                return;
            }
        };

        let Some(resource) = self.current_resource() else {
            self.show_warning("No resource selected");
            return;
        };

        let Some(item) = self.selected_item().cloned() else {
            self.show_warning("No item selected");
            return;
        };

        let action_name = resource
            .actions
            .get(action_index)
            .map(|a| a.display_name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        self.loading = true;
        self.mode = Mode::Normal;

        match execute_action(&self.client, resource, action_index, &item).await {
            Ok(_) => {
                // Action succeeded - refresh to see updated state
                self.refresh().await;
            }
            Err(e) => {
                self.error = Some(format!("Action '{}' failed: {}", action_name, e));
            }
        }

        self.loading = false;
    }
}
