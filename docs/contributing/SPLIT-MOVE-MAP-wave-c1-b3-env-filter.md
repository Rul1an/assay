# SPLIT MOVE MAP - Wave C1 B3 (env_filter.rs)

## Section Move Map

- public types + filter implementation (`EnvMode`, `EnvFilterResult`, `EnvFilter`, impl blocks) -> `crates/assay-cli/src/env_filter/engine.rs`
- glob matching helpers (`matches_any_pattern`, `matches_pattern`) -> `crates/assay-cli/src/env_filter/matcher.rs`
- pattern constants (`SAFE_BASE_PATTERNS`, `SECRET_SCRUB_PATTERNS`, `EXEC_INFLUENCE_PATTERNS`) -> `crates/assay-cli/src/env_filter/patterns.rs`
- unit tests -> `crates/assay-cli/src/env_filter/tests.rs`
- module docs + re-export facade -> `crates/assay-cli/src/env_filter/mod.rs`

## Symbol Map (old -> new)

- `EnvMode` -> `env_filter/engine.rs`
- `EnvFilterResult` -> `env_filter/engine.rs`
- `EnvFilter` + filter methods -> `env_filter/engine.rs`
- `matches_any_pattern` -> `env_filter/matcher.rs`
- `SAFE_BASE_PATTERNS` -> `env_filter/patterns.rs`
- `SECRET_SCRUB_PATTERNS` -> `env_filter/patterns.rs`
- `EXEC_INFLUENCE_PATTERNS` -> `env_filter/patterns.rs`

## Facade Contract

`crates/assay-cli/src/env_filter/mod.rs` re-exports the same public surface as the previous single-file module.
