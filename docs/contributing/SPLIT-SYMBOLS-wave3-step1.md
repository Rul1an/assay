# Wave 3 Step 1 symbol inventory snapshot

Snapshot commit:
- `e0baca5e6bbc93d447979108739012e339aa28f6`

Commands:

```bash
rg -n "^[[:space:]]*(pub|pub\\(crate\\))\\s" crates/assay-cli/src/cli/commands/monitor.rs
rg -n "^[[:space:]]*(pub|pub\\(crate\\))\\s" crates/assay-core/src/providers/trace.rs
```

Output:

```text
== monitor
37:pub struct MonitorArgs {
40:    pub pid: Vec<u32>,
44:    pub ebpf: Option<PathBuf>,
48:    pub print: bool,
52:    pub quiet: bool,
56:    pub duration: Option<humantime::Duration>,
60:    pub policy: Option<PathBuf>,
64:    pub monitor_all: bool,
67:pub async fn run(args: MonitorArgs) -> anyhow::Result<i32> {

== trace
13:pub struct TraceClient {
372:    pub fn from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
```
