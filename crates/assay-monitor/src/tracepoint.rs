use anyhow::{Context, Result};
use assay_common::{
    KEY_OFFSET_FILENAME, KEY_OFFSET_FILENAME_OPENAT2, KEY_OFFSET_FORK_CHILD,
    KEY_OFFSET_FORK_PARENT, KEY_OFFSET_OPENAT2_HOW, KEY_OFFSET_OPENAT_FLAGS,
    KEY_OFFSET_OPENAT_MODE, KEY_OFFSET_SENDMSG_MSGHDR, KEY_OFFSET_SENDTO_SOCKADDR,
    KEY_OFFSET_SOCKADDR, KEY_OFFSET_SYSCALL_EXIT_RET,
};
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

        let openat_fn = Self::find_offset("syscalls", "sys_enter_openat", "filename").unwrap_or(24);
        map.insert(KEY_OFFSET_FILENAME, openat_fn);

        let connect_sa = Self::find_offset("syscalls", "sys_enter_connect", "uservaddr")
            .or_else(|_| Self::find_offset("syscalls", "sys_enter_connect", "addr"))
            .unwrap_or(24);
        map.insert(KEY_OFFSET_SOCKADDR, connect_sa);

        let fork_parent =
            Self::find_offset("sched", "sched_process_fork", "parent_pid").unwrap_or(24);
        map.insert(KEY_OFFSET_FORK_PARENT, fork_parent);

        let fork_child =
            Self::find_offset("sched", "sched_process_fork", "child_pid").unwrap_or(44);
        map.insert(KEY_OFFSET_FORK_CHILD, fork_child);

        let openat2_fn =
            Self::find_offset("syscalls", "sys_enter_openat2", "filename").unwrap_or(24);
        map.insert(KEY_OFFSET_FILENAME_OPENAT2, openat2_fn);

        let openat_flags = Self::find_offset("syscalls", "sys_enter_openat", "flags").unwrap_or(32);
        map.insert(KEY_OFFSET_OPENAT_FLAGS, openat_flags);

        let openat_mode = Self::find_offset("syscalls", "sys_enter_openat", "mode").unwrap_or(40);
        map.insert(KEY_OFFSET_OPENAT_MODE, openat_mode);

        let openat2_how = Self::find_offset("syscalls", "sys_enter_openat2", "how").unwrap_or(32);
        map.insert(KEY_OFFSET_OPENAT2_HOW, openat2_how);

        let exit_ret = Self::find_offset("syscalls", "sys_exit_openat", "ret").unwrap_or(16);
        map.insert(KEY_OFFSET_SYSCALL_EXIT_RET, exit_ret);

        let sendto_sa = Self::find_offset("syscalls", "sys_enter_sendto", "addr")
            .or_else(|_| Self::find_offset("syscalls", "sys_enter_sendto", "dest_addr"))
            .or_else(|_| Self::find_offset("syscalls", "sys_enter_sendto", "uservaddr"))
            .unwrap_or(48);
        map.insert(KEY_OFFSET_SENDTO_SOCKADDR, sendto_sa);

        let sendmsg_msg = Self::find_offset("syscalls", "sys_enter_sendmsg", "msg")
            .or_else(|_| Self::find_offset("syscalls", "sys_enter_sendmsg", "msg_ptr"))
            .unwrap_or(24);
        map.insert(KEY_OFFSET_SENDMSG_MSGHDR, sendmsg_msg);

        map
    }

    /// Reads tracepoint format file, checking tracefs first then debugfs.
    pub fn find_offset(category: &str, event: &str, field: &str) -> Result<u32> {
        Self::parse_format_file(&Self::format_path(category, event)?, field)
    }

    /// Offset of a `__data_loc` field, refusing any other declaration.
    ///
    /// A `__data_loc` field is a 4-byte descriptor whose low 16 bits locate the payload within the
    /// record and whose high 16 bits give its length. Reading a plain field at that offset as such
    /// a descriptor would point the probe at arbitrary record bytes, so a layout that is not
    /// exactly that shape is an error rather than a guess.
    pub fn find_data_loc_offset(category: &str, event: &str, field: &str) -> Result<u32> {
        Self::parse_data_loc_format_file(&Self::format_path(category, event)?, field)
    }

    fn format_path(category: &str, event: &str) -> Result<String> {
        let potential_roots = ["/sys/kernel/tracing", "/sys/kernel/debug/tracing"];

        for root in potential_roots {
            let path = format!("{}/events/{}/{}/format", root, category, event);
            if Path::new(&path).exists() {
                return Ok(path);
            }
        }

        Err(anyhow::anyhow!(
            "Tracepoint format file not found for {}/{}",
            category,
            event
        ))
    }

    fn parse_format_file(path: &str, field_name: &str) -> Result<u32> {
        Ok(Self::find_field(path, field_name)?.offset)
    }

    fn parse_data_loc_format_file(path: &str, field_name: &str) -> Result<u32> {
        let field = Self::find_field(path, field_name)?;
        if !field.declaration.starts_with("__data_loc ") || field.size != Some(4) {
            return Err(anyhow::anyhow!(
                "Field '{}' in '{}' is not a 4-byte __data_loc field: '{}' size {:?}",
                field_name,
                path,
                field.declaration,
                field.size
            ));
        }
        Ok(field.offset)
    }

    fn find_field(path: &str, field_name: &str) -> Result<FormatField> {
        let content = fs::read_to_string(path)?;

        for line in content.lines() {
            // Format: field:const char *filename; offset:16; size:8; signed:0;
            let Some(rest) = line.trim().strip_prefix("field:") else {
                continue;
            };
            let mut parts = rest.split(';').map(str::trim);
            let declaration = parts.next().unwrap_or("");
            if Self::declared_name(declaration) != field_name {
                continue;
            }

            let mut offset = None;
            let mut size = None;
            for part in parts {
                if let Some(value) = part.strip_prefix("offset:") {
                    offset = Some(value.parse::<u32>().context("Failed to parse offset")?);
                } else if let Some(value) = part.strip_prefix("size:") {
                    size = Some(value.parse::<u32>().context("Failed to parse size")?);
                }
            }
            let offset = offset
                .with_context(|| format!("Field '{}' in '{}' has no offset", field_name, path))?;
            return Ok(FormatField {
                declaration: declaration.to_string(),
                offset,
                size,
            });
        }

        Err(anyhow::anyhow!(
            "Field '{}' not found in format file '{}'",
            field_name,
            path
        ))
    }

    /// The field name is the declaration's last token, without pointer stars or an array suffix:
    /// `const char * filename`, `char parent_comm[16]` and `__data_loc char[] filename` name
    /// `filename`, `parent_comm` and `filename`.
    fn declared_name(declaration: &str) -> &str {
        let last = declaration.split_whitespace().last().unwrap_or("");
        let without_array = last.split('[').next().unwrap_or(last);
        without_array.trim_start_matches('*')
    }
}

struct FormatField {
    declaration: String,
    offset: u32,
    size: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::TracepointResolver;
    use std::path::PathBuf;

    /// `sched/sched_process_exec` as Linux 6.8 prints it: the filename is a `__data_loc` field,
    /// whose declaration puts `[]` before the field name rather than after it.
    const SCHED_PROCESS_EXEC: &str = "name: sched_process_exec
ID: 300
format:
\tfield:unsigned short common_type;\toffset:0;\tsize:2;\tsigned:0;
\tfield:unsigned char common_flags;\toffset:2;\tsize:1;\tsigned:0;
\tfield:unsigned char common_preempt_count;\toffset:3;\tsize:1;\tsigned:0;
\tfield:int common_pid;\toffset:4;\tsize:4;\tsigned:1;

\tfield:__data_loc char[] filename;\toffset:8;\tsize:4;\tsigned:0;
\tfield:pid_t pid;\toffset:12;\tsize:4;\tsigned:1;
\tfield:pid_t old_pid;\toffset:16;\tsize:4;\tsigned:1;

print fmt: \"filename=%s pid=%d old_pid=%d\", __get_str(filename), REC->pid, REC->old_pid
";

    const SYS_ENTER_OPENAT: &str = "name: sys_enter_openat
format:
\tfield:unsigned short common_type;\toffset:0;\tsize:2;\tsigned:0;
\tfield:int __syscall_nr;\toffset:8;\tsize:4;\tsigned:1;
\tfield:int dfd;\toffset:16;\tsize:8;\tsigned:0;
\tfield:const char * filename;\toffset:24;\tsize:8;\tsigned:0;
\tfield:int flags;\toffset:32;\tsize:8;\tsigned:0;
";

    const SCHED_PROCESS_FORK: &str = "name: sched_process_fork
format:
\tfield:char parent_comm[16];\toffset:8;\tsize:16;\tsigned:0;
\tfield:pid_t parent_pid;\toffset:24;\tsize:4;\tsigned:1;
";

    fn with_format<T>(name: &str, content: &str, read: impl FnOnce(&str) -> T) -> T {
        let path: PathBuf = std::env::temp_dir().join(format!(
            "assay-monitor-tp-format-{}-{name}",
            std::process::id()
        ));
        std::fs::write(&path, content).expect("write format fixture");
        let result = read(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        result
    }

    fn parse(name: &str, content: &str, field: &str) -> anyhow::Result<u32> {
        with_format(name, content, |path| {
            TracepointResolver::parse_format_file(path, field)
        })
    }

    fn parse_data_loc(name: &str, content: &str, field: &str) -> anyhow::Result<u32> {
        with_format(name, content, |path| {
            TracepointResolver::parse_data_loc_format_file(path, field)
        })
    }

    #[test]
    fn data_loc_array_field_is_found_by_its_name() {
        assert_eq!(
            parse("exec-filename", SCHED_PROCESS_EXEC, "filename").unwrap(),
            8
        );
    }

    #[test]
    fn data_loc_element_type_is_not_mistaken_for_the_field_name() {
        assert!(parse("exec-char", SCHED_PROCESS_EXEC, "char").is_err());
    }

    #[test]
    fn pointer_and_array_suffix_fields_keep_resolving() {
        assert_eq!(
            parse("openat-filename", SYS_ENTER_OPENAT, "filename").unwrap(),
            24
        );
        assert_eq!(
            parse("fork-comm", SCHED_PROCESS_FORK, "parent_comm").unwrap(),
            8
        );
        assert_eq!(
            parse("fork-pid", SCHED_PROCESS_FORK, "parent_pid").unwrap(),
            24
        );
    }

    #[test]
    fn exec_filename_resolves_as_a_four_byte_data_loc_descriptor() {
        assert_eq!(
            parse_data_loc("exec-loc", SCHED_PROCESS_EXEC, "filename").unwrap(),
            8
        );
    }

    #[test]
    fn data_loc_lookup_refuses_a_plain_field() {
        let error = parse_data_loc("exec-pid", SCHED_PROCESS_EXEC, "pid").unwrap_err();
        assert!(error.to_string().contains("__data_loc"), "{error}");
        assert!(parse_data_loc("openat-ptr", SYS_ENTER_OPENAT, "filename").is_err());
    }

    #[test]
    fn data_loc_lookup_refuses_an_unexpected_descriptor_size() {
        let widened = SCHED_PROCESS_EXEC.replace("offset:8;\tsize:4;", "offset:8;\tsize:8;");
        assert_ne!(widened, SCHED_PROCESS_EXEC, "the fixture edit must apply");
        assert!(parse_data_loc("exec-size", &widened, "filename").is_err());
    }

    #[test]
    fn data_loc_lookup_refuses_a_missing_field() {
        assert!(parse_data_loc("exec-missing", SCHED_PROCESS_FORK, "filename").is_err());
    }
}
