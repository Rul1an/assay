# Wave G1 Step 1 Checklist

- [ ] `PayloadSandboxDegraded` is typed and no longer free-text-led
- [ ] only the two frozen fallback paths emit `assay.sandbox.degraded`
- [ ] intentional audit/permissive mode emits no degradation event
- [ ] fail-closed denial emits no degradation event
- [ ] degradation observations are deduplicated per `(component, reason_code)`
- [ ] sandbox profiling writes an evidence-profile sidecar for `assay evidence export`
- [ ] evidence export maps `sandbox_degradations` to `assay.sandbox.degraded`
- [ ] healthy runs do not emit false-positive degradation events
- [ ] `A5-002` is documented as no longer a pure signal gap
- [ ] docs do not claim sandbox correctness, guaranteed containment, or broad telemetry coverage
