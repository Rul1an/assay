## Field Matrix

- trace spans: 2
- archive SDK events: 0
- manifest-digest binding: tamper-evident-match
- tool_call_id join: archive-side-absent

| Field | L1 Trace | L2 Archive | Join | Claim class | Notes |
|---|---|---|---|---|---|
| run identity (run_id) | run_arm_c_20260524T205020Z_3 | run_arm_c_20260524T205020Z_3 | assay.run.id | correlation |  |
| archive schema | assay.runner.archive_manifest.v0 | assay.runner.archive_manifest.v0 | assay.archive.schema | provenance |  |
| manifest digest binding | sha256:1b32b4613ebc23e679ee4309234ba7efdd6ea2090b946f4ad2620fb051124594 | sha256:1b32b4613ebc23e679ee4309234ba7efdd6ea2090b946f4ad2620fb051124594 | tamper-evident-match | tamper-evident binding |  |
| GenAI provider | openai | n/a (trace-side concept) | span | provenance |  |
| GenAI request model | absent | n/a | span | provenance |  |
| GenAI response model | absent | n/a | span | provenance |  |
| GenAI input tokens | absent | n/a (archive does not measure tokens) | span | cost/context |  |
| GenAI output tokens | absent | n/a | span | cost/context |  |
| tool names | read_file | absent | tool name | joinable behavior |  |
| tool_call_id joinability | tc_runner_policy_001 | absent | archive-side-absent | primary join key |  |
| filesystem paths | n/a (not in trace contract) | present | none | measured effect; bounded negative if health is clean | /opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T205020Z_3/trace.json, /opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T205020Z_3/workdir/openai-agents-input.txt, /opt/actions-runner/_work/assay/assay/docs/experiments/runner-vs-otel-2026-05/workload/dist/manifest-binding.js |
| network endpoints | n/a (not in trace contract) | empty | none | measured effect |  |
| process execs | n/a | empty | none | measured effect |  |
| policy decisions | n/a (unless custom) | empty | tool_call_id when present | enforcement |  |
| ringbuf_drops | absent | 0 | archive health | measurement integrity |  |
| cgroup correlation status | absent | clean | assay.runner.correlation_status | measurement integrity |  |
