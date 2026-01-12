use std::fs;
use std::path::Path;
use anyhow::{Result, Context};
use std::collections::HashMap;

/// Resolves field offsets forsyscall tracepoints by reading the kernel's format file.
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

        let openat_fn = Self::find_offset("syscalls", "sys_enter_openat", "filename").unwrap_or(24);
        map.insert(0, openat_fn);

        let connect_sa = Self::find_offset("syscalls", "sys_enter_connect", "uservaddr").unwrap_or(24);
        // Note: Field name for connect might be different on some kernels (uservaddr vs addr)
        // If uservaddr fails, try addr? For now keep simple fallback.
        map.insert(1, connect_sa);

        let fork_parent = Self::find_offset("sched", "sched_process_fork", "parent_pid").unwrap_or(24);
        map.insert(2, fork_parent);

        let fork_child = Self::find_offset("sched", "sched_process_fork", "child_pid").unwrap_or(44);
        map.insert(3, fork_child);

        let openat2_fn = Self::find_offset("syscalls", "sys_enter_openat2", "filename").unwrap_or(24);
        map.insert(4, openat2_fn);

        map
    }

    /// Reads /sys/kernel/debug/tracing/events/<category>/<name>/format and finds the offset of <field>.
        Self::parse_format_file(&path, field)
    }

    fn parse_format_file(path: &str, field_name: &str) -> Result<u32> {
        let content = fs::read_to_string(path)?;

        for line in content.lines() {
            let line = line.trim();
            // Format: field:const char *filename; offset:16; size:8; signed:0;
            if line.starts_with("field:") {
                let parts: Vec<&str> = line.split(';').collect();
                if parts.len() < 2 { continue; }

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

        Err(anyhow::anyhow!("Field '{}' not found in format file '{}'", field_name, path))
    }
}
