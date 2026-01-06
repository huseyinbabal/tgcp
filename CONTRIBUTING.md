# Contributing to tgcp

Thank you for your interest in contributing to tgcp! This document provides guidelines and information for contributors.

## Before You Start

**Important:** Before adding a new GCP service or major feature, please start a discussion in our [GitHub Discussions](https://github.com/huseyinbabal/tgcp/discussions) board. This helps us:

- Avoid duplicate work
- Discuss the best approach
- Ensure the feature aligns with project goals
- Get community feedback

## How to Contribute

1. **Fork the repository**
2. **Create your feature branch** (`git checkout -b feature/amazing-feature`)
3. **Commit your changes** (`git commit -m 'Add some amazing feature'`)
4. **Push to the branch** (`git push origin feature/amazing-feature`)
5. **Open a Pull Request**

## Development Setup

```bash
# Clone your fork
git clone https://github.com/YOUR_USERNAME/tgcp.git
cd tgcp

# Build the project
cargo build

# Run in development mode
cargo run

# Run tests
cargo test

# Check formatting
cargo fmt --check

# Run linter
cargo clippy
```

## Architecture

tgcp follows a data-driven architecture where GCP resource definitions are stored as JSON configuration files. This makes it easy to add new resource types without writing extensive code.

```
src/
├── resources/          # JSON resource definitions (one per service)
│   ├── compute.json
│   ├── storage.json
│   ├── gke.json
│   └── ...
├── resource/
│   ├── registry.rs     # Resource registry and loading
│   └── mod.rs
├── gcp/
│   ├── client.rs       # GCP HTTP client management
│   ├── auth.rs         # Authentication (ADC, service accounts, metadata)
│   └── dispatch.rs     # API dispatch and response handling
└── ui/
    ├── header.rs       # Header with context info
    ├── help.rs         # Help screen
    ├── dialog.rs       # Confirmation dialogs
    └── ...
```

### Lightweight Design

tgcp uses a custom lightweight HTTP client with native GCP authentication instead of heavy SDKs. This results in:

- **Fast builds** - Minimal dependencies
- **Small binary** - Optimized release binary
- **No gcloud dependency** - Works standalone with native auth

## Adding a New GCP Service

To add support for a new GCP service, follow these steps:

### 1. Start a Discussion

Before writing any code, [open a discussion](https://github.com/huseyinbabal/tgcp/discussions/new?category=ideas) to propose the new service. Include:

- Which GCP service you want to add
- Which resources/operations you plan to support
- Why this service would be valuable

### 2. Add Resource JSON Definition

Create `src/resources/myservice.json`:

```json
{
  "resources": {
    "myservice-items": {
      "display_name": "MyService Items",
      "service": "myservice",
      "api": {
        "base": "https://myservice.googleapis.com/v1",
        "path": "projects/{project}/locations/{zone}/items",
        "method": "GET"
      },
      "response_path": "items",
      "id_field": "name",
      "name_field": "displayName",
      "columns": [
        { "header": "Name", "json_path": "displayName", "width": 25 },
        { "header": "Status", "json_path": "state", "width": 12, "color_map": "status" },
        { "header": "Created", "json_path": "createTime", "width": 20 }
      ],
      "actions": [
        {
          "display_name": "Delete",
          "api": {
            "method": "DELETE",
            "path": "projects/{project}/locations/{zone}/items/{name}"
          },
          "shortcut": "ctrl+d",
          "confirm": {
            "message": "Delete item '{name}'?",
            "destructive": true
          }
        }
      ]
    }
  }
}
```

### 3. Register the Resource File

Add the new JSON file to `src/resource/registry.rs`:

```rust
const RESOURCE_FILES: &[&str] = &[
    // ... existing files
    include_str!("../resources/myservice.json"),
];
```

### 4. Test Your Changes

```bash
# Build and run
cargo run

# Run tests to ensure JSON is valid
cargo test

# Test the new resource
# Press : and type your resource name
```

## JSON Resource Definition Reference

### Required Fields

| Field | Description |
|-------|-------------|
| `display_name` | Human-readable name shown in UI |
| `service` | GCP service identifier |
| `api.base` | Base URL for the API |
| `api.path` | API endpoint path (supports `{project}`, `{zone}`, `{name}` placeholders) |
| `api.method` | HTTP method (GET, POST, DELETE, etc.) |
| `response_path` | JSON path to extract items from response |
| `id_field` | Field to use as unique identifier |
| `name_field` | Field to use as display name |
| `columns` | Array of column definitions |

### Optional Fields

| Field | Description |
|-------|-------------|
| `actions` | Array of action definitions (start, stop, delete, etc.) |
| `sub_resources` | Array of child resource definitions |
| `color_map` | Reference to color map for status fields |

### Action Definition

```json
{
  "display_name": "Action Name",
  "api": {
    "method": "POST",
    "path": "projects/{project}/zones/{zone}/items/{name}:action"
  },
  "shortcut": "a",
  "confirm": {
    "message": "Perform action on '{name}'?",
    "destructive": false
  }
}
```

### Sub-Resource Definition

```json
{
  "resource_key": "child-items",
  "display_name": "Child Items",
  "shortcut": "c",
  "parent_id_field": "name",
  "filter_param": "parent"
}
```

## Reserved Keyboard Shortcuts

Do not use these shortcuts in your resource actions:

| Shortcut | Reserved For |
|----------|--------------|
| `d` | Describe |
| `g` | Part of `gg` (go to top) |
| `G` | Go to bottom |
| `j/k` | Navigation |
| `r` | Refresh |
| `q` | Quit |
| `?` | Help |
| `:` | Command mode |
| `/` | Filter |
| `Backspace` | Navigate back |

## Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Pass all clippy lints (`cargo clippy`)
- Write descriptive commit messages
- Add comments for complex logic

## Pull Request Guidelines

- Keep PRs focused on a single feature or fix
- Update documentation if needed
- Ensure all tests pass
- Add tests for new functionality
- Reference any related issues or discussions

## Questions?

If you have questions, feel free to:

- Open a [Discussion](https://github.com/huseyinbabal/tgcp/discussions)
- Check existing issues and PRs

Thank you for contributing!
