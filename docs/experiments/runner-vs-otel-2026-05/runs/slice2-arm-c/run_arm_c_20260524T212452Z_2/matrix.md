## Field Matrix

- trace spans: 2
- archive SDK events: 3
- manifest-digest binding: tamper-evident-match
- tool_call_id join: joined:tc_runner_policy_001

| Field | L1 Trace | L2 Archive | Join | Claim class | Notes |
|---|---|---|---|---|---|
| run identity (run_id) | run_arm_c_20260524T212452Z_2 | run_arm_c_20260524T212452Z_2 | assay.run.id | correlation |  |
| archive schema | assay.runner.archive_manifest.v0 | assay.runner.archive_manifest.v0 | assay.archive.schema | provenance |  |
| manifest digest binding | sha256:9810efb669946b7572b1cad63c41d0f990e2dd345da1642609c299e906a77c26 | sha256:9810efb669946b7572b1cad63c41d0f990e2dd345da1642609c299e906a77c26 | tamper-evident-match | tamper-evident binding |  |
| GenAI provider | openai | n/a (trace-side concept) | span | provenance |  |
| GenAI request model | absent | n/a | span | provenance |  |
| GenAI response model | absent | n/a | span | provenance |  |
| GenAI input tokens | absent | n/a (archive does not measure tokens) | span | cost/context |  |
| GenAI output tokens | absent | n/a | span | cost/context |  |
| tool names | read_file | read_file | tool name | joinable behavior |  |
| tool_call_id joinability | tc_runner_policy_001 | tc_runner_policy_001 | joined:tc_runner_policy_001 | primary join key |  |
| filesystem paths | n/a (not in trace contract) | present | none | measured effect; bounded negative if health is clean | /opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T212452Z_2/sdk-events.ndjson, /opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T212452Z_2/trace.json, /opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T212452Z_2/workdir/openai-agents-input.txt |
| network endpoints | n/a (not in trace contract) | empty | none | measured effect |  |
| process execs | n/a | empty | none | measured effect |  |
| policy decisions | n/a (unless custom) | empty | tool_call_id when present | enforcement |  |
| ringbuf_drops | absent | 0 | archive health | measurement integrity |  |
| cgroup correlation status | absent | clean | assay.runner.correlation_status | measurement integrity |  |
