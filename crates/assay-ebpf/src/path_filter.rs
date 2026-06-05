use super::DATA_LEN;

#[inline(always)]
pub(super) fn is_loader_telemetry_open_path(path: &[u8; DATA_LEN]) -> bool {
    // Dynamic linker and libc config probes flooded delegated runs without
    // carrying runner-spike attribution evidence.
    bytes_start_with(path, b"/etc/ld.so.cache\0")
        || bytes_start_with(path, b"/etc/localtime\0")
        || bytes_start_with(path, b"/etc/ssl/openssl.cnf\0")
        // Python runtime bootstrap files are control-plane noise from the MCP
        // fixture process, not agent file access.
        || bytes_start_with(path, b"/usr/pyvenv.cfg\0")
        // System library and locale lookups dominated the openat stream and
        // varied with loader state across otherwise identical fixtures.
        || bytes_start_with(path, b"/lib/")
        || bytes_start_with(path, b"/lib32/")
        || bytes_start_with(path, b"/lib64/")
        || bytes_start_with(path, b"/usr/bin/pyvenv.cfg\0")
        || bytes_start_with(path, b"/usr/bin/python3._pth\0")
        || bytes_start_with(path, b"/usr/bin/python3.12._pth\0")
        || bytes_start_with(path, b"/usr/bin/pybuilddir.txt\0")
        // The OpenAI Agents fixture's vendored dependency tree is SDK runtime
        // plumbing; SDK evidence is recorded from the normalized SDK layer.
        || bytes_start_with(
            path,
            b"/opt/actions-runner/_work/assay/assay/runner-fixtures/openai-agents/node_modules",
        )
        || bytes_start_with(path, b"/usr/local/lib/")
        || bytes_start_with(path, b"/usr/local/share/locale/")
        || bytes_start_with(path, b"/usr/lib/")
        || bytes_start_with(path, b"/usr/share/locale/")
        // Kernel and device introspection paths are monitor/runtime plumbing,
        // not filesystem capability evidence for the fixture.
        || bytes_start_with(path, b"/proc/")
        || bytes_start_with(path, b"/sys/")
        || bytes_start_with(path, b"/dev/")
}

#[inline(always)]
fn bytes_start_with(path: &[u8; DATA_LEN], prefix: &[u8]) -> bool {
    for index in 0..DATA_LEN {
        if index >= prefix.len() {
            return true;
        }
        if path[index] != prefix[index] {
            return false;
        }
    }
    false
}
