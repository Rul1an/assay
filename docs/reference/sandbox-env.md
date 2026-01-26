# Environment Filtering Reference

Complete reference for Assay's environment variable filtering system.

---

## Overview

Assay scrubs environment variables before spawning sandboxed processes to prevent credential leakage and execution hijacking. This is **enabled by default** with no configuration required.

---

## Filtering Modes

| Mode | CLI Flag | Security Level | Use Case |
|------|----------|----------------|----------|
| **Pattern Scrub** | (default) | Medium | Development, trusted code |
| **Strict** | `--env-strict` | High | CI/CD, untrusted code |
| **Passthrough** | `--env-passthrough` | None | Debugging only |

---

## Pattern Scrub (Default)

### Scrubbed Patterns

Variables matching these patterns are **removed**:

#### Credentials & Secrets

| Pattern | Examples |
|---------|----------|
| `*_TOKEN` | `GITHUB_TOKEN`, `SLACK_TOKEN`, `NPM_TOKEN` |
| `*_SECRET` | `AWS_SECRET_ACCESS_KEY`, `CLIENT_SECRET` |
| `*_KEY` | `OPENAI_API_KEY`, `STRIPE_API_KEY` |
| `*_PASSWORD` | `DB_PASSWORD`, `REDIS_PASSWORD` |
| `*_CREDENTIAL*` | `GCP_CREDENTIALS`, `AZURE_CREDENTIAL_FILE` |

#### Cloud Providers

| Pattern | Examples |
|---------|----------|
| `AWS_*` | `AWS_ACCESS_KEY_ID`, `AWS_SESSION_TOKEN` |
| `OPENAI_*` | `OPENAI_API_KEY`, `OPENAI_ORG_ID` |
| `ANTHROPIC_*` | `ANTHROPIC_API_KEY` |
| `AZURE_*` | `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID` |
| `GCP_*` | `GCP_PROJECT`, `GCP_SERVICE_ACCOUNT` |
| `GOOGLE_*` | `GOOGLE_APPLICATION_CREDENTIALS` |

#### Version Control & CI

| Pattern | Examples |
|---------|----------|
| `GITHUB_*` | `GITHUB_TOKEN`, `GITHUB_SHA` |
| `GITLAB_*` | `GITLAB_TOKEN`, `GITLAB_CI` |
| `BITBUCKET_*` | `BITBUCKET_TOKEN` |
| `CI_*` | `CI_JOB_TOKEN`, `CI_REGISTRY_PASSWORD` |

#### Databases & Storage

| Pattern | Examples |
|---------|----------|
| `*_DATABASE_URL` | `DATABASE_URL`, `MONGO_DATABASE_URL` |
| `*_CONNECTION_STRING` | `POSTGRES_CONNECTION_STRING` |
| `REDIS_*` | `REDIS_URL`, `REDIS_PASSWORD` |
| `MONGO_*` | `MONGO_URI`, `MONGO_PASSWORD` |

#### Security Tools

| Pattern | Examples |
|---------|----------|
| `SSH_*` | `SSH_AUTH_SOCK`, `SSH_AGENT_PID` |
| `GPG_*` | `GPG_TTY`, `GPG_AGENT_INFO` |
| `VAULT_*` | `VAULT_TOKEN`, `VAULT_ADDR` |
| `KUBECONFIG` | `KUBECONFIG` |

#### Execution Influence

| Pattern | Risk | Examples |
|---------|------|----------|
| `LD_PRELOAD` | Library injection | `LD_PRELOAD=/tmp/evil.so` |
| `LD_LIBRARY_PATH` | Library path hijack | `LD_LIBRARY_PATH=/tmp` |
| `LD_AUDIT` | Audit library injection | |
| `LD_DEBUG` | Debug output leak | |
| `DYLD_*` | macOS library injection | `DYLD_INSERT_LIBRARIES` |
| `PYTHONPATH` | Python module injection | `PYTHONPATH=/tmp` |
| `PYTHONSTARTUP` | Python startup script | |
| `PYTHONHOME` | Python installation hijack | |
| `NODE_OPTIONS` | Node.js flag injection | `NODE_OPTIONS=--require=/tmp/evil.js` |
| `NODE_PATH` | Node module path hijack | |
| `RUBYOPT` | Ruby option injection | |
| `RUBYLIB` | Ruby library path hijack | |
| `PERL5LIB` | Perl library path hijack | |
| `PERL5OPT` | Perl option injection | |
| `JAVA_TOOL_OPTIONS` | JVM option injection | |
| `_JAVA_OPTIONS` | JVM option injection | |
| `CLASSPATH` | Java classpath hijack | |
| `RUSTC_WRAPPER` | Rust compiler wrapper hijack | |
| `CC`, `CXX` | Compiler hijack | |
| `CFLAGS`, `LDFLAGS` | Compiler flag injection | |

### Allowed Patterns (Safe Base)

These variables **pass through** by default:

#### System Essentials

| Pattern | Examples |
|---------|----------|
| `PATH` | `PATH` |
| `HOME` | `HOME` |
| `USER` | `USER` |
| `SHELL` | `SHELL` |
| `LOGNAME` | `LOGNAME` |

#### Locale & Terminal

| Pattern | Examples |
|---------|----------|
| `LANG` | `LANG` |
| `LC_*` | `LC_ALL`, `LC_CTYPE`, `LC_MESSAGES` |
| `TERM` | `TERM` |
| `COLORTERM` | `COLORTERM` |
| `CLICOLOR` | `CLICOLOR` |
| `NO_COLOR` | `NO_COLOR` |

#### Temporary Directories

| Pattern | Examples |
|---------|----------|
| `TMPDIR` | `TMPDIR` |
| `TMP` | `TMP` |
| `TEMP` | `TEMP` |

#### XDG Directories

| Pattern | Examples |
|---------|----------|
| `XDG_*` | `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_RUNTIME_DIR` |

#### Development Tools

| Pattern | Examples |
|---------|----------|
| `RUST_LOG` | `RUST_LOG` |
| `RUST_BACKTRACE` | `RUST_BACKTRACE` |
| `CARGO_*` | `CARGO_HOME`, `CARGO_TARGET_DIR` |
| `EDITOR` | `EDITOR` |
| `VISUAL` | `VISUAL` |
| `PAGER` | `PAGER` |

---

## Strict Mode

With `--env-strict`, **only** safe base patterns pass through. All other variables are scrubbed.

```bash
assay sandbox --env-strict -- ./server
```

### What Passes in Strict Mode

- `PATH`, `HOME`, `USER`, `SHELL`, `LOGNAME`
- `LANG`, `LC_*`, `TERM`, `COLORTERM`
- `TMPDIR`, `TMP`, `TEMP` (set to scoped dir)
- `XDG_*`
- `RUST_LOG`, `RUST_BACKTRACE`, `CARGO_*`
- `EDITOR`, `VISUAL`, `PAGER`
- `NO_COLOR`, `CLICOLOR`

### What's Blocked in Strict Mode

Everything else, including:
- Custom application config vars (`MY_APP_CONFIG`)
- Development shortcuts (`DEBUG=1`)
- Non-secret project vars (`PROJECT_NAME`)

### Explicit Allow

Use `--env-allow` to pass specific vars through strict mode:

```bash
assay sandbox --env-strict \
  --env-allow MY_CONFIG \
  --env-allow DEBUG \
  -- ./server
```

---

## Passthrough Mode

⚠️ **DANGER**: Disables all environment filtering.

```bash
assay sandbox --env-passthrough -- ./server
```

**Use only for**:
- Debugging scrubbing issues
- Trusted code in controlled environments
- When you understand the risks

**Never use for**:
- Untrusted code
- CI/CD pipelines
- Production environments

---

## Banner Output

The sandbox banner shows environment filtering status:

### Pattern Scrub
```
Env: scrubbed (42 passed, 7 removed)
```

### Strict Mode
```
Env: strict (12 passed, 47 scrubbed)
```

### Passthrough Mode
```
Env: ⚠ passthrough (59 vars, DANGER)
```

---

## Programmatic API

For the Rust SDK:

```rust
use assay::env_filter::{EnvFilter, EnvMode};

// Default pattern scrub
let filter = EnvFilter::default();
let result = filter.filter(&std::env::vars().collect());

println!("Passed: {}", result.passed_count);
println!("Scrubbed: {:?}", result.scrubbed_keys);

// Strict mode
let filter = EnvFilter::strict();

// With explicit allows
let filter = EnvFilter::default()
    .with_allowed(&["MY_VAR", "OTHER_VAR"]);
```

---

## Common Questions

### Why is my env var being scrubbed?

Check if it matches any scrub pattern:
- Contains `TOKEN`, `SECRET`, `KEY`, `PASSWORD`
- Starts with `AWS_`, `GITHUB_`, `OPENAI_`, etc.
- Is an execution-influence var (`LD_*`, `PYTHONPATH`, etc.)

### How do I allow a specific variable?

```bash
assay sandbox --env-allow MY_VAR -- ./server
```

### Why can't I use `--env-allow *`?

For security. If you need all vars, use `--env-passthrough` (with caution).

### What about `.env` files?

Assay doesn't read `.env` files. Variables must be in the process environment. Consider:

```bash
# Load .env, then sandbox
export $(cat .env | xargs)
assay sandbox --env-allow VAR1 --env-allow VAR2 -- ./server
```

### Can I customize the scrub patterns?

Not yet. Future versions may support policy-based env rules. For now:
- Use `--env-allow` for specific allows
- Use `--env-strict` for maximum security

---

## Security Rationale

### Why Default Scrub?

**Threat**: MCP servers and AI agents may be:
- Malicious by design
- Compromised via supply chain
- Tricked by prompt injection

**Risk**: Environment variables contain secrets that could be:
- Exfiltrated to attacker servers
- Used to access cloud resources
- Logged or cached insecurely

**Mitigation**: Remove secrets before the process can access them.

### Why Execution-Influence Scrubbing?

**Threat**: Variables like `LD_PRELOAD` allow code injection:

```bash
# Attacker sets:
export LD_PRELOAD=/tmp/keylogger.so

# Victim runs (unknowingly loads malicious code):
./innocent-program
```

**Risk**: Even "trusted" programs can be hijacked.

**Mitigation**: Strip all execution-influence variables by default.

### Why Strict Mode?

**Problem**: Pattern matching can miss:
- Custom secret names (`MY_API_KEY_V2`)
- New cloud provider prefixes
- Internal credential conventions

**Solution**: Default-deny everything, explicit-allow only what's needed.

---

## See Also

- [assay sandbox CLI Reference](cli/sandbox.md)
- [Sandbox Security Guide](../guides/sandbox-security.md)
- [Sandbox Policies Reference](sandbox-policies.md)
