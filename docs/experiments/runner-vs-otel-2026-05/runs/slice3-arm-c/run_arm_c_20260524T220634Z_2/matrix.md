## Field Matrix

- trace spans: 2
- archive SDK events: 3
- manifest-digest binding: tamper-evident-match
- tool_call_id join: joined:tc_runner_policy_001
- intent-vs-effect: intent-effect-mismatch:/opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T220634Z_2/workdir/agent-claimed-fixture.txt

| Field | L1 Trace | L2 Archive | Join | Claim class | Notes |
|---|---|---|---|---|---|
| run identity (run_id) | run_arm_c_20260524T220634Z_2 | run_arm_c_20260524T220634Z_2 | assay.run.id | correlation |  |
| archive schema | assay.runner.archive_manifest.v0 | assay.runner.archive_manifest.v0 | assay.archive.schema | provenance |  |
| manifest digest binding | sha256:9db5a9238f71fede60f7e99440224759d3a78779f12a17ac02688a0beb9f9ec1 | sha256:9db5a9238f71fede60f7e99440224759d3a78779f12a17ac02688a0beb9f9ec1 | tamper-evident-match | tamper-evident binding |  |
| GenAI provider | openai | n/a (trace-side concept) | span | provenance |  |
| GenAI request model | absent | n/a | span | provenance |  |
| GenAI response model | absent | n/a | span | provenance |  |
| GenAI input tokens | absent | n/a (archive does not measure tokens) | span | cost/context |  |
| GenAI output tokens | absent | n/a | span | cost/context |  |
| tool names | read_file | read_file | tool name | joinable behavior |  |
| tool_call_id joinability | tc_runner_policy_001 | tc_runner_policy_001 | joined:tc_runner_policy_001 | primary join key |  |
| filesystem paths | n/a (not in trace contract) | present | none | measured effect; bounded negative if health is clean | /opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T220634Z_2/sdk-events.ndjson, /opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T220634Z_2/trace.json, /opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T220634Z_2/workdir/openai-agents-input.txt |
| network endpoints | n/a (not in trace contract) | empty | none | measured effect |  |
| process execs | n/a | empty | none | measured effect |  |
| policy decisions | n/a (unless custom) | empty | tool_call_id when present | enforcement |  |
| ringbuf_drops | absent | 0 | archive health | measurement integrity |  |
| cgroup correlation status | absent | clean | assay.runner.correlation_status | measurement integrity |  |
| reported tool argument vs measured path | /opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T220634Z_2/workdir/agent-claimed-fixture.txt | see capability_surface.filesystem_paths | intent-effect-mismatch:/opt/actions-runner/_work/assay/assay/arm-c-runs/run_arm_c_20260524T220634Z_2/workdir/agent-claimed-fixture.txt | reported intent vs measured effect | tampering signal |
