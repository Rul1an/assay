use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Resolves field offsets for syscall tracepoints by reading the kernel's format file.
pub struct TracepointResolver;

impl TracepointResolver {
    /// Resolves common offsets for openat and connect syscalls.
    /// Returns a map of CONFIG keys to offsets.
    pub fn resolve_default_offsets() -> HashMap<u32, u32> {
        let mut map = HashMap::new();

        // Key 0: openat filename
        // Key 1: connect sockaddr
        // Key 2: fork parent
        // Key 3: fork child
        // Key 4: openat2 filename (SOTA)
        // Key 5: openat flags
        // Key 6: openat mode
        // Key 7: openat2 how
        // Key 8: syscall exit return value

        let openat_fn = Self::find_offset("syscalls", "sys_enter_openat", "filename").unwrap_or(24);
        map.insert(0, openat_fn);

        let connect_sa = Self::find_offset("syscalls", "sys_enter_connect", "uservaddr")
            .or_else(|_| Self::find_offset("syscalls", "sys_enter_connect", "addr"))
            .unwrap_or(24);
        map.insert(1, connect_sa);

        let fork_parent =
            Self::find_offset("sched", "sched_process_fork", "parent_pid").unwrap_or(24);
        map.insert(2, fork_parent);

        let fork_child =
            Self::find_offset("sched", "sched_process_fork", "child_pid").unwrap_or(44);
        map.insert(3, fork_child);

        let openat2_fn =
            Self::find_offset("syscalls", "sys_enter_openat2", "filename").unwrap_or(24);
        map.insert(4, openat2_fn);

        let openat_flags = Self::find_offset("syscalls", "sys_enter_openat", "flags").unwrap_or(32);
        map.insert(5, openat_flags);

        let openat_mode = Self::find_offset("syscalls", "sys_enter_openat", "mode").unwrap_or(40);
        map.insert(6, openat_mode);

        let openat2_how = Self::find_offset("syscalls", "sys_enter_openat2", "how").unwrap_or(32);
        map.insert(7, openat2_how);

        let exit_ret = Self::find_offset("syscalls", "sys_exit_openat", "ret").unwrap_or(16);
        map.insert(8, exit_ret);

        map
    }

    /// Reads tracepoint format file, checking tracefs first then debugfs.
    pub fn find_offset(category: &str, event: &str, field: &str) -> Result<u32> {
        let potential_roots = ["/sys/kernel/tracing", "/sys/kernel/debug/tracing"];

        for root in potential_roots {
            let path = format!("{}/events/{}/{}/format", root, category, event);
            if Path::new(&path).exists() {
                return Self::parse_format_file(&path, field);
            }
        }

        Err(anyhow::anyhow!(
            "Tracepoint format file not found for {}/{}",
            category,
            event
        ))
    }

    fn parse_format_file(path: &str, field_name: &str) -> Result<u32> {
        let content = fs::read_to_string(path)?;

        for line in content.lines() {
            let line = line.trim();
            // Format: field:const char *filename; offset:16; size:8; signed:0;
            if line.starts_with("field:") {
                let parts: Vec<&str> = line.split(';').collect();
                if parts.len() < 2 {
                    continue;
                }

                // Parse "field:..." part to extract name
                // "field:const char *filename" -> last token is "filename" or "*filename"
                let declaration = parts[0].strip_prefix("field:").unwrap_or("").trim();
                // Remove array brackets if present
                let decl_clean = declaration.split('[').next().unwrap_or(declaration);
                // Get last token (variable name)
                let actual_name = decl_clean.split_whitespace().last().unwrap_or("");
                // Remove pointer indirection
                let actual_name = actual_name.trim_start_matches('*');

                if actual_name == field_name {
                    // Found it! Parse offset.
                    // Search for "offset:N" part
                    for part in parts.iter().skip(1) {
                        let part = part.trim();
                        if part.starts_with("offset:") {
                            let val_str = part.strip_prefix("offset:").unwrap_or("0");
                            let val = val_str.parse::<u32>().context("Failed to parse offset")?;
                            return Ok(val);
                        }
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "Field '{}' not found in format file '{}'",
            field_name,
            path
        ))
    }
}
