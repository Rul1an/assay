## Field Matrix

- trace spans: 2
- archive SDK events: 3
- manifest-digest binding: trace-attribute-absent
- tool_call_id join: joined:tc_runner_policy_001

| Field | L1 Trace | L2 Archive | Join | Claim class | Notes |
|---|---|---|---|---|---|
| run identity (run_id) | run_arm_b_20260524T194017Z_2 | run_fixture_001 | assay.run.id | correlation |  |
| archive schema | absent | assay.runner.archive_manifest.v0 | assay.archive.schema | provenance |  |
| manifest digest binding | absent | sha256:c76eb655e4630235ad137a50427a47db4b70ab9dcb40ddf30ad3f3165ee9d1d8 | trace-attribute-absent | tamper-evident binding |  |
| GenAI provider | openai | n/a (trace-side concept) | span | provenance |  |
| GenAI request model | absent | n/a | span | provenance |  |
| GenAI response model | absent | n/a | span | provenance |  |
| GenAI input tokens | absent | n/a (archive does not measure tokens) | span | cost/context |  |
| GenAI output tokens | absent | n/a | span | cost/context |  |
| tool names | read_file | read_file | tool name | joinable behavior |  |
| tool_call_id joinability | tc_runner_policy_001 | tc_runner_policy_001 | joined:tc_runner_policy_001 | primary join key |  |
| filesystem paths | n/a (not in trace contract) | present | none | measured effect; bounded negative if health is clean | /tmp/fixture/openai-agents-input.txt |
| network endpoints | n/a (not in trace contract) | empty | none | measured effect |  |
| process execs | n/a | present | none | measured effect |  |
| policy decisions | n/a (unless custom) | present | tool_call_id when present | enforcement |  |
| ringbuf_drops | absent | 0 | archive health | measurement integrity |  |
| cgroup correlation status | absent | clean | assay.runner.correlation_status | measurement integrity |  |
