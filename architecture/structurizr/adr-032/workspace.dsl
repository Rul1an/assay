workspace "Assay ADR-032 MCP Policy Stack" "C4/Structurizr workspace for the ADR-032 MCP policy enforcement and evidence line through Wave42." {

    properties {
        structurizr.inspection.* warning
        structurizr.inspection.model.relationship.technology info
        structurizr.inspection.model.softwaresystem.decisions info
    }

    !identifiers hierarchical
    !docs docs

    model {
        mcpClient = person "Agent / MCP Client" "An agent or MCP client that issues tool calls through Assay-governed runtime paths."

        authSource = softwareSystem "Transport Auth / Resource Server" "Owns transport authentication and resource-server concerns outside Assay." {
            tags "External"
        }

        approvalSource = softwareSystem "Approval Artifact Source" "Provides approval artifacts and approval-state inputs to the runtime path." {
            tags "External"
        }

        policySource = softwareSystem "Policy Bundle Source" "Provides typed policy bundles and bounded obligation declarations consumed by Assay." {
            tags "External"
        }

        consumers = softwareSystem "Replay / Consumer Readers" "CLI, replay, reporting, and downstream readers that consume decision and evidence payloads." {
            tags "External"
        }

        assay = softwareSystem "Assay MCP Policy Stack" "Runtime policy enforcement and evidence layer for MCP tool calls under ADR-032." {
            tags "Assay"

            policyRuntime = container "Policy Runtime" "Intercepts MCP tool-call paths, evaluates policy, executes bounded obligations, and produces typed decisions." "Rust runtime / library path" {
                pepHook = component "PEP Hook" "Intercepts MCP runtime calls and hands them into policy evaluation." "Rust module"
                contextEnvelope = component "Context Envelope" "Collects lane, principal, auth summaries, approval state, and other bounded policy inputs." "Rust module"
                evaluator = component "PDP / Evaluator" "Evaluates typed policy rules against runtime context." "Rust module"
                decisionModel = component "Typed Decision Model" "Normalizes evaluation results into typed decision variants." "Rust module"
                obligationExecutor = component "Bounded Obligation Executor" "Executes only the bounded obligation paths landed by Waves25-Wave42." "Rust module"
                failClosedSelector = component "Fail-Closed Selector" "Selects typed fail-closed fallback outcomes separate from policy and enforcement deny origins." "Rust module"
                evidenceEmitter = component "Evidence Emitter" "Emits decision events, obligation outcomes, and additive evidence metadata." "Rust module"
            }

            evidenceLayer = container "Evidence and Replay Layer" "Normalizes evidence, builds replay basis payloads, and supports deterministic downstream reads." "Rust runtime / library path" {
                decisionProjection = component "Decision Projection" "Projects normalized decision and evidence payloads for downstream consumers." "Rust module"
                replayBasis = component "Replay Diff Basis" "Provides deterministic replay and diff payloads across policy revisions." "Rust module"
                consumerReadLayer = component "Consumer Read Layer" "Applies deterministic read precedence and compatibility handling for payload consumers." "Rust module"
                contextCompleteness = component "Context Completeness Metadata" "Marks complete, partial, or absent envelope state for payload consumers." "Rust module"
            }
        }

        mcpClient -> assay.policyRuntime.pepHook "Issues MCP tool calls through governed runtime paths" "MCP"
        authSource -> assay.policyRuntime.contextEnvelope "Supplies transport-auth summaries and runtime auth context" "Context summary"
        approvalSource -> assay.policyRuntime.contextEnvelope "Supplies approval state and artifact-bound inputs" "Approval state"
        policySource -> assay.policyRuntime.evaluator "Supplies typed rules and obligations" "Policy bundle"

        assay.policyRuntime.pepHook -> assay.policyRuntime.contextEnvelope "Builds runtime context envelope" "In-process call"
        assay.policyRuntime.contextEnvelope -> assay.policyRuntime.evaluator "Supplies policy inputs" "In-process call"
        assay.policyRuntime.evaluator -> assay.policyRuntime.decisionModel "Produces evaluation result" "In-process call"
        assay.policyRuntime.decisionModel -> assay.policyRuntime.obligationExecutor "Carries allow/obligation semantics" "In-process call"
        assay.policyRuntime.decisionModel -> assay.policyRuntime.failClosedSelector "Carries deny/fallback semantics" "In-process call"
        assay.policyRuntime.obligationExecutor -> assay.policyRuntime.evidenceEmitter "Reports bounded enforcement outcomes" "In-process call"
        assay.policyRuntime.failClosedSelector -> assay.policyRuntime.evidenceEmitter "Reports fail-closed outcomes" "In-process call"
        assay.policyRuntime.decisionModel -> assay.policyRuntime.evidenceEmitter "Reports typed decision metadata" "In-process call"

        assay.policyRuntime.evidenceEmitter -> assay.evidenceLayer.decisionProjection "Projects normalized decision/evidence payloads" "In-process call"
        assay.evidenceLayer.decisionProjection -> assay.evidenceLayer.replayBasis "Builds replay/diff basis" "In-process call"
        assay.evidenceLayer.decisionProjection -> assay.evidenceLayer.consumerReadLayer "Feeds downstream compatibility and read precedence" "In-process call"
        assay.evidenceLayer.contextCompleteness -> assay.evidenceLayer.consumerReadLayer "Annotates envelope completeness" "In-process call"

        assay.evidenceLayer.replayBasis -> consumers "Supplies replay and diff payloads" "JSON evidence"
        assay.evidenceLayer.consumerReadLayer -> consumers "Supplies deterministic consumer-facing payloads" "JSON evidence"
    }

    views {
        systemContext assay "ADR032SystemContext" "System context for the ADR-032 MCP policy stack." {
            include *
            autoLayout lr
        }

        container assay "ADR032Containers" "Container view for the ADR-032 MCP policy stack." {
            include *
            autoLayout lr
        }

        component assay.policyRuntime "ADR032PolicyRuntimeComponents" "Component view of the policy runtime container." {
            include *
            autoLayout lr
        }

        component assay.evidenceLayer "ADR032EvidenceComponents" "Component view of the evidence and replay layer." {
            include *
            autoLayout lr
        }

        styles {
            element "Person" {
                background "#1168bd"
                color "#ffffff"
                shape Person
            }
            element "Software System" {
                background "#2d882d"
                color "#ffffff"
            }
            element "Container" {
                background "#438dd5"
                color "#ffffff"
            }
            element "Component" {
                background "#85bbf0"
                color "#000000"
            }
            element "External" {
                background "#999999"
                color "#ffffff"
                border Dashed
            }
            element "Assay" {
                background "#0b7285"
                color "#ffffff"
            }
        }
    }
}
