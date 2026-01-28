# Architecture Diagrams

This document contains visual representations of the Assay architecture using Mermaid diagrams.

## System Overview

```mermaid
flowchart TB
    subgraph UserLayer[User Layer]
        Dev[Developer]
        CI[CI/CD Pipeline]
        Agent[AI Agent]
    end

    subgraph InterfaceLayer[Interface Layer]
        CLI[assay-cli]
        PySDK[Python SDK]
        MCPServer[assay-mcp-server]
    end

    subgraph CoreLayer[Core Layer]
        Core[assay-core]
        Metrics[assay-metrics]
        Policy[assay-policy]
    end

    subgraph RuntimeLayer[Runtime Layer]
        Monitor[assay-monitor]
        eBPF[assay-ebpf]
        Kernel[Linux Kernel]
    end

    subgraph DataLayer[Data Layer]
        Store[(SQLite Store)]
        Traces[Trace Files]
        Policies[Policy Files]
    end

    Dev --> CLI
    Dev --> PySDK
    CI --> CLI
    Agent --> MCPServer

    CLI --> Core
    PySDK --> Core
    MCPServer --> Core

    Core --> Metrics
    Core --> Policy
    Core --> Store

    MCPServer --> Policy
    Monitor --> Policy
    Monitor --> eBPF
    eBPF --> Kernel

    CLI --> Traces
    CLI --> Policies
    Core --> Traces
```

## Component Architecture

```mermaid
graph TB
    subgraph Core[assay-core]
        Engine[engine::Runner]
        Storage[storage::Store]
        MCP[mcp::]
        Trace[trace::]
        Report[report::]
        Providers[providers::]
        MetricsAPI[metrics_api::]
    end

    subgraph CLI[assay-cli]
        Main[main.rs]
        Dispatch[dispatch]
        Commands[commands::]
        BuildRunner[build_runner]
    end

    subgraph Metrics[assay-metrics]
        MustContain[MustContain]
        Semantic[SemanticSimilarity]
        Regex[RegexMatch]
        JsonSchema[JsonSchema]
        ArgsValid[ArgsValid]
        SequenceValid[SequenceValid]
    end

    subgraph MCP[assay-mcp-server]
        Server[MCP Server]
        Proxy[Proxy Handler]
    end

    subgraph Monitor[assay-monitor]
        MonitorCore[Monitor Core]
        EventStream[Event Stream]
    end

    Main --> Dispatch
    Dispatch --> Commands
    Commands --> BuildRunner
    BuildRunner --> Engine
    Engine --> Storage
    Engine --> Providers
    Engine --> MetricsAPI
    MetricsAPI --> Metrics
    Engine --> Report

    Server --> Proxy
    Proxy --> MCP
    MCP --> Policy

    MonitorCore --> EventStream
    MonitorCore --> eBPF

    style Core fill:#fff4e1
    style CLI fill:#e1f5ff
    style Metrics fill:#e8f5e9
    style MCP fill:#f3e5f5
    style Monitor fill:#ffebee
```

## Data Flow: Test Execution

```mermaid
sequenceDiagram
    participant User
    participant CLI as assay-cli
    participant Runner as Runner
    participant Store as Store
    participant Cache as VcrCache
    participant LLM as LLM Client
    participant Metrics as Metrics
    participant Report as Report

    User->>CLI: assay run --config assay.yaml
    CLI->>CLI: load_config()
    CLI->>Store: Store::open()
    CLI->>CLI: build_runner()
    CLI->>Runner: Runner::new()
    CLI->>Runner: run_suite()

    loop For each test
        Runner->>Store: create_run()
        Runner->>Cache: lookup()
        alt Cache hit
            Cache-->>Runner: Cached result
        else Cache miss
            Runner->>LLM: complete()
            LLM-->>Runner: Response
            Runner->>Metrics: evaluate()
            Metrics-->>Runner: Score
            Runner->>Cache: store()
        end
        Runner->>Store: insert_result()
    end

    Runner->>Report: format()
    Report-->>CLI: Output
    CLI-->>User: Results
```

## Policy Enforcement Flow

```mermaid
flowchart TD
    ToolCall[Tool Call Request] --> MCP[MCP Server]
    MCP --> Parse[Parse JSON-RPC]
    Parse --> Policy[Load Policy]
    Policy --> Tier1{Tier 1 Check?}
    Tier1 -->|Yes| Kernel[Kernel/LSM Check]
    Tier1 -->|No| Tier2[Tier 2 Check]
    Kernel -->|Block| Reject[Reject]
    Kernel -->|Allow| Tier2
    Tier2 -->|Block| Reject
    Tier2 -->|Allow| Forward[Forward to Tool]
    Forward --> Execute[Execute Tool]
    Execute --> Response[Return Response]
    Reject --> Error[Return Error]
    Response --> Audit[Audit Log]
    Error --> Audit
```

## Trace Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Record: Agent executes
    Record --> JSONL: Write trace
    JSONL --> Import: assay import
    Import --> Ingest: Trace ingest
    Ingest --> Store: Store in DB
    Store --> Precompute: Precompute embeddings
    Precompute --> Ready: Ready for replay
    Ready --> Replay: assay run
    Replay --> Evaluate: Evaluate metrics
    Evaluate --> Report: Generate report
    Report --> [*]

    Ready --> Upgrade: Schema upgrade
    Upgrade --> Store
```

## Evidence Pipeline

```mermaid
flowchart TD
    subgraph Capture[Capture Phase]
        Profile[Profile Collector]
        Events[Native Events]
    end

    subgraph Transform[Transform Phase]
        Mapper[EvidenceMapper]
        CloudEvents[CloudEvents v1.0]
    end

    subgraph Bundle[Bundle Phase]
        Writer[BundleWriter]
        JCS[JCS Canonicalization]
        Hash[SHA-256 Content ID]
    end

    subgraph Verify[Verification Phase]
        Reader[BundleReader]
        Integrity[Integrity Check]
        Lint[Lint Rules]
    end

    Profile --> Events
    Events --> Mapper
    Mapper --> CloudEvents
    CloudEvents --> Writer
    Writer --> JCS
    JCS --> Hash
    Hash --> TarGz[bundle.tar.gz]

    TarGz --> Reader
    Reader --> Integrity
    Integrity --> Lint
    Lint --> SARIF[SARIF Report]

    style Capture fill:#e1f5ff
    style Transform fill:#fff4e1
    style Bundle fill:#e8f5e9
    style Verify fill:#f3e5f5
```

## MCP Integration Architecture

```mermaid
graph LR
    subgraph Agent[AI Agent]
        AgentCode[Agent Code]
    end

    subgraph Assay[Assay MCP Server]
        MCPProxy[MCP Proxy]
        PolicyEngine[Policy Engine]
        AuditLog[Audit Logger]
    end

    subgraph Tools[Tool Servers]
        Tool1[Tool Server 1]
        Tool2[Tool Server 2]
        ToolN[Tool Server N]
    end

    AgentCode -->|JSON-RPC| MCPProxy
    MCPProxy --> PolicyEngine
    PolicyEngine -->|Allow| MCPProxy
    PolicyEngine -->|Deny| AgentCode
    MCPProxy -->|Forward| Tool1
    MCPProxy -->|Forward| Tool2
    MCPProxy -->|Forward| ToolN
    Tool1 -->|Response| MCPProxy
    Tool2 -->|Response| MCPProxy
    ToolN -->|Response| MCPProxy
    MCPProxy -->|Response| AgentCode
    MCPProxy --> AuditLog
```

## Runtime Security Architecture

```mermaid
graph TB
    subgraph Userspace[Userspace]
        Monitor[assay-monitor]
        Policy[assay-policy]
        Config[Policy Config]
    end

    subgraph Kernel[Linux Kernel]
        eBPF[assay-ebpf Program]
        LSM[LSM Hooks]
        Tracepoints[Tracepoints]
    end

    subgraph Process[Monitored Process]
        Agent[AI Agent Process]
        Syscalls[System Calls]
    end

    Config --> Policy
    Policy --> Monitor
    Monitor --> eBPF
    eBPF --> LSM
    eBPF --> Tracepoints
    Agent --> Syscalls
    Syscalls --> Tracepoints
    Tracepoints --> eBPF
    eBPF -->|Block/Allow| Syscalls
    eBPF --> Monitor
    Monitor -->|Log| Audit
```

## Storage Schema

```mermaid
erDiagram
    RUNS ||--o{ RESULTS : has
    RUNS {
        int run_id PK
        string suite
        string status
        timestamp created_at
    }
    RESULTS ||--o{ ATTEMPTS : has
    RESULTS {
        int result_id PK
        int run_id FK
        string test_id
        string status
        float score
        string fingerprint
        json details
    }
    ATTEMPTS {
        int attempt_id PK
        int result_id FK
        int attempt_num
        string status
        json response
    }
    EMBEDDINGS {
        int embedding_id PK
        string fingerprint
        vector embedding
    }
    JUDGE_CACHE {
        string cache_key PK
        json result
    }
```

## Metrics Evaluation Flow

```mermaid
flowchart TD
    TestCase[TestCase] --> LoadMetrics[Load Metrics]
    LoadMetrics --> ForEach[For Each Metric]
    ForEach --> GetResponse[Get LLM Response]
    GetResponse --> GetExpected[Get Expected Value]
    GetExpected --> Evaluate[Evaluate Metric]
    Evaluate --> Content{Content Metric?}
    Evaluate --> Semantic{Semantic Metric?}
    Evaluate --> Structure{Structure Metric?}
    Content --> MustContain[MustContain]
    Content --> Regex[RegexMatch]
    Semantic --> Embed[Generate Embedding]
    Embed --> Similarity[Calculate Similarity]
    Structure --> JsonSchema[JSON Schema Validate]
    Structure --> ArgsValid[Args Valid]
    MustContain --> Score[Calculate Score]
    Regex --> Score
    Similarity --> Score
    JsonSchema --> Score
    ArgsValid --> Score
    Score --> Aggregate[Aggregate Scores]
    Aggregate --> Result[Test Result]
```

## CI/CD Integration Flow

```mermaid
flowchart LR
    PR[Pull Request] --> Trigger[CI Trigger]
    Trigger --> Checkout[Checkout Code]
    Checkout --> Install[Install Assay]
    Install --> Load[Load Config + Traces]
    Load --> Run[assay run]
    Run --> Runner[Runner Executes]
    Runner --> Results[Test Results]
    Results --> Format[Format Output]
    Format --> SARIF[SARIF Report]
    Format --> JUnit[JUnit Report]
    SARIF --> Upload[Upload to GitHub]
    JUnit --> TestReport[Test Reporting]
    Results --> ExitCode{Exit Code}
    ExitCode -->|0| Pass[Pass: Allow Merge]
    ExitCode -->|1| Fail[Fail: Block PR]
    Fail --> Comment[PR Comment]
```

## Policy Compilation Flow

```mermaid
flowchart TD
    PolicyYAML[policy.yaml] --> Parse[Parse YAML]
    Parse --> Validate[Validate Schema]
    Validate --> Compile[Compile Policy]
    Compile --> Split[Split into Tiers]
    Split --> Tier1[Tier 1: Kernel Rules]
    Split --> Tier2[Tier 2: Userspace Rules]
    Tier1 --> Exact[Exact Paths]
    Tier1 --> CIDR[CIDR Blocks]
    Tier1 --> Ports[Port Numbers]
    Tier2 --> Glob[Glob Patterns]
    Tier2 --> Regex[Regex Patterns]
    Tier2 --> Complex[Complex Constraints]
    Exact --> Compiled[CompiledPolicy]
    CIDR --> Compiled
    Ports --> Compiled
    Glob --> Compiled
    Regex --> Compiled
    Complex --> Compiled
    Compiled --> Deploy[Deploy to Runtime]
```

## Python SDK Architecture

```mermaid
graph TB
    subgraph Python[Python Layer]
        Client[AssayClient]
        Coverage[Coverage]
        Explainer[Explainer]
        Pytest[Pytest Plugin]
    end

    subgraph Rust[Rust Layer]
        PyO3[PyO3 Bindings]
        Core[assay-core]
    end

    subgraph Native[Native Layer]
        TraceIngest[trace::ingest]
        CoverageAnalyze[coverage::analyze]
        Explain[explain::explain]
    end

    Client --> PyO3
    Coverage --> PyO3
    Explainer --> PyO3
    Pytest --> Client
    PyO3 --> Core
    Core --> TraceIngest
    Core --> CoverageAnalyze
    Core --> Explain
```

## Error Handling Flow

```mermaid
flowchart TD
    Error[Error Occurs] --> Classify[Classify Error]
    Classify --> ConfigError[Config Error]
    Classify --> TestError[Test Error]
    Classify --> PolicyError[Policy Error]
    Classify --> SystemError[System Error]

    ConfigError --> Diagnostic[Generate Diagnostic]
    TestError --> ErrorPolicy{Error Policy?}
    PolicyError --> Violation[Policy Violation]
    SystemError --> Log[Log Error]

    ErrorPolicy -->|Block| Fail[Fail Test]
    ErrorPolicy -->|Allow| Warn[Warn + Continue]
    ErrorPolicy -->|Retry| Retry[Retry Test]

    Diagnostic --> ExitCode2[Exit Code 2]
    Violation --> ExitCode1[Exit Code 1]
    Fail --> ExitCode1
    Warn --> Continue[Continue]
    Retry --> Test[Re-run Test]
    Log --> ExitCode2
```

## Related Documentation

- [Codebase Overview](codebase-overview.md) - Detailed component descriptions
- [Interdependencies](interdependencies.md) - Dependency relationships
- [User Flows](user-flows.md) - User journey mappings
