# feat(cli): add `assay mcp config-path` helper

## 🚀 Description
This PR addresses the "15-minute success" friction by introducing `assay mcp config-path`.
Instead of requiring users to manually construct complex JSON configs for Claude Desktop, this command:
1.  **Auto-detects** the OS-specific configuration path for Claude Desktop (and Cursor).
2.  **Generates** a secure, copy-paste ready `mcpServers` configuration snippet.
3.  **Verifies** if the config file and policy file exist.

## 📋 Changes
-   **New Command**: `assay mcp config-path <client>`
-   **New Dependency**: `dirs` (v5.0) for cross-platform path detection.
-   **Architecture**: Added `config_path.rs` module in `assay-cli`.
-   **Cleanup**: Removed legacy/unused imports.

## 📸 Example Usage
```bash
$ assay mcp config-path claude

┌─ Claude Desktop Configuration
│
│  Config file: /Users/roel/Library/Application Support/Claude/claude_desktop_config.json
│  Status: ✓ Found
│
├─ Add this to your mcpServers:
│
│  {
│    "filesystem-secure": { ... }
│  }
```

## 🛡️ Security
-   No automatic file modification (read-only detection).
-   Privacy-safe (runs locally, no telemetry).

## ✅ Checklist
- [x] Code compiles (`cargo check`)
- [x] Formatting (`cargo fmt`)
- [x] Linting (`cargo clippy`)
- [x] Unit tests added/passed
