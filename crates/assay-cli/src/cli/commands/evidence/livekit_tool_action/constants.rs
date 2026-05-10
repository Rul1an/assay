pub(super) const EVENT_TYPE: &str = "assay.receipt.livekit.tool_action.v1";
pub(super) const EVENT_SOURCE: &str = "urn:assay:external:livekit:function-tool-call";
pub(super) const RECEIPT_SCHEMA: &str = "assay.receipt.livekit.tool-action.v1";
pub(super) const SOURCE_SYSTEM: &str = "livekit_agents";
pub(super) const SOURCE_SURFACE: &str = "function_tools_executed";
pub(super) const REDUCER_VERSION: &str = "assay-livekit-function-tools-executed@0.1.0";
pub(super) const INPUT_SCHEMA: &str = "livekit.function-tools-executed.export.v1";
pub(super) const DEFAULT_RUN_ID: &str = "import-livekit-tool-action";
pub(super) const MAX_NAME_CHARS: usize = 160;
pub(super) const MAX_REF_CHARS: usize = 240;

pub(super) const REQUIRED_TOP_LEVEL_KEYS: &[&str] = &[
    "schema",
    "framework",
    "surface",
    "runtime_mode",
    "event_ref",
    "created_at",
    "function_calls",
    "function_call_outputs",
];

pub(super) const OPTIONAL_TOP_LEVEL_KEYS: &[&str] =
    &["type", "has_tool_reply", "has_agent_handoff"];

pub(super) const CALL_KEYS: &[&str] = &[
    "id",
    "type",
    "call_id",
    "name",
    "arguments",
    "arguments_ref",
    "created_at",
    "group_id",
];

pub(super) const OUTPUT_KEYS: &[&str] = &[
    "id",
    "type",
    "call_id",
    "name",
    "output",
    "output_ref",
    "is_error",
    "created_at",
];

pub(super) const FORBIDDEN_TOP_LEVEL_KEYS: &[(&str, &str)] = &[
    (
        "transcript",
        "artifact: transcript import is out of scope for LiveKit tool-action v1",
    ),
    (
        "audio",
        "artifact: audio import is out of scope for LiveKit tool-action v1",
    ),
    (
        "user_input",
        "artifact: raw user input is out of scope for LiveKit tool-action v1",
    ),
    (
        "model_output",
        "artifact: raw model output is out of scope for LiveKit tool-action v1",
    ),
    (
        "usage",
        "artifact: usage telemetry is out of scope for LiveKit tool-action v1",
    ),
    (
        "latency",
        "artifact: latency telemetry is out of scope for LiveKit tool-action v1",
    ),
    (
        "room_state",
        "artifact: room state is out of scope for LiveKit tool-action v1",
    ),
    (
        "participant_identity",
        "artifact: participant identity is out of scope for LiveKit tool-action v1",
    ),
    (
        "capture_context",
        "artifact: capture context and session identity are out of scope for LiveKit tool-action v1",
    ),
    (
        "trace",
        "artifact: full trace payloads are out of scope for LiveKit tool-action v1",
    ),
    (
        "spans",
        "artifact: full span payloads are out of scope for LiveKit tool-action v1",
    ),
];
