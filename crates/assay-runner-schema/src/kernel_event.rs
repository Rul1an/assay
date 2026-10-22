use assay_common::{
    EVENT_CONNECT, EVENT_CONNECT_BLOCKED, EVENT_EXEC, EVENT_FILE_BLOCKED, EVENT_OPENAT,
    EVENT_SENDMSG, EVENT_SENDTO,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::EffectDimension;

/// Kernel events written before v1. Kept so retained archives still verify; see
/// [`open_operation_flags`] for the label mapping that version is frozen to.
pub const KERNEL_EVENT_SCHEMA: &str = "assay.runner.kernel_event.v0";
/// Kernel events whose `operation_flags` follow the Linux open(2) bits
/// ([`open_operation_flags_v1`]).
pub const KERNEL_EVENT_SCHEMA_V1: &str = "assay.runner.kernel_event.v1";

pub const KNOWN_KERNEL_EVENT_KEYS: [&str; 18] = [
    "schema",
    "run_id",
    "seq",
    "pid",
    "event_type",
    "kind",
    "value",
    "flags",
    "mode",
    "resolve",
    "return_value",
    "access_mode",
    "operation_flags",
    "status",
    "cgroup_id",
    "network_destination",
    "network_port",
    "rule_id",
];

pub const REQUIRED_KERNEL_EVENT_KEYS: [&str; 7] = [
    "schema",
    "run_id",
    "seq",
    "pid",
    "event_type",
    "kind",
    "value",
];

pub const MAX_KERNEL_EXACT_COUNT: u64 = 9_007_199_254_740_991;
pub const MAX_KERNEL_EXACT_SIGNED_INT: i64 = 9_007_199_254_740_991;

#[must_use]
pub fn is_known_kernel_event_key(key: &str) -> bool {
    KNOWN_KERNEL_EVENT_KEYS.contains(&key)
}

#[must_use]
pub fn is_safe_run_id(run_id: &str) -> bool {
    run_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[must_use]
pub fn open_access_mode(flags: u64) -> &'static str {
    match flags & 0o3 {
        0 => "read",
        1 => "write",
        2 => "read_write",
        _ => "unknown",
    }
}

/// The `kernel_event.v0` label mapping, frozen so that retained v0 archives keep
/// verifying. Its `"exclusive"` label tests bit `0o400`, which on Linux is
/// `O_NOCTTY`, not `O_EXCL` (`0o200`): in v0, `"exclusive"` does not mean an
/// exclusive create, and an `O_EXCL` open carries no label for it. The raw
/// `flags` value is authoritative in both versions.
#[must_use]
pub fn open_operation_flags(flags: u64) -> Vec<String> {
    let mut out = Vec::new();
    if flags & 0o100 != 0 {
        out.push("create".to_string());
    }
    if flags & 0o1000 != 0 {
        out.push("truncate".to_string());
    }
    if flags & 0o2000 != 0 {
        out.push("append".to_string());
    }
    if flags & 0o400 != 0 {
        out.push("exclusive".to_string());
    }
    out
}

/// The `kernel_event.v1` label mapping: Linux `O_CREAT` (`0o100`), `O_TRUNC`
/// (`0o1000`), `O_APPEND` (`0o2000`) and `O_EXCL` (`0o200`).
#[must_use]
pub fn open_operation_flags_v1(flags: u64) -> Vec<String> {
    let mut out = Vec::new();
    if flags & 0o100 != 0 {
        out.push("create".to_string());
    }
    if flags & 0o1000 != 0 {
        out.push("truncate".to_string());
    }
    if flags & 0o2000 != 0 {
        out.push("append".to_string());
    }
    if flags & 0o200 != 0 {
        out.push("exclusive".to_string());
    }
    out
}

#[must_use]
pub fn open_status(return_value: i64) -> &'static str {
    if return_value < 0 {
        "error"
    } else {
        "success"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum KernelEventValidationError {
    #[error("kernel event schema must be {KERNEL_EVENT_SCHEMA} or {KERNEL_EVENT_SCHEMA_V1}")]
    UnknownSchema,
    #[error("invalid kernel event shape")]
    InputShape,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelLayerEvent {
    pub schema: String,
    pub run_id: String,
    pub seq: u64,
    pub pid: u32,
    pub event_type: u32,
    pub kind: String,
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroup_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_destination: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolve: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_value: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_mode: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub operation_flags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl KernelLayerEvent {
    pub fn validate_emitted(&self) -> Result<EffectDimension, KernelEventValidationError> {
        let operation_flags_for: fn(u64) -> Vec<String> = match self.schema.as_str() {
            KERNEL_EVENT_SCHEMA => open_operation_flags,
            KERNEL_EVENT_SCHEMA_V1 => open_operation_flags_v1,
            _ => return Err(KernelEventValidationError::UnknownSchema),
        };
        if self.run_id.is_empty()
            || !is_safe_run_id(&self.run_id)
            || self.seq > MAX_KERNEL_EXACT_COUNT
            || self.value.as_deref().is_some_and(str::is_empty)
        {
            return Err(KernelEventValidationError::InputShape);
        }

        match (self.event_type, self.kind.as_str()) {
            (EVENT_OPENAT, "openat") => {
                if !self.has_no_blocked_connect_fields() {
                    return Err(KernelEventValidationError::InputShape);
                }
                let (Some(flags), Some(mode), Some(return_value), Some(access_mode), Some(status)) = (
                    self.flags,
                    self.mode,
                    self.return_value,
                    self.access_mode.as_deref(),
                    self.status.as_deref(),
                ) else {
                    return Err(KernelEventValidationError::InputShape);
                };
                if flags > MAX_KERNEL_EXACT_COUNT
                    || mode > MAX_KERNEL_EXACT_COUNT
                    || !(-MAX_KERNEL_EXACT_SIGNED_INT..=MAX_KERNEL_EXACT_SIGNED_INT)
                        .contains(&return_value)
                    || self
                        .resolve
                        .is_some_and(|resolve| resolve == 0 || resolve > MAX_KERNEL_EXACT_COUNT)
                {
                    return Err(KernelEventValidationError::InputShape);
                }
                if access_mode != open_access_mode(flags)
                    || self.operation_flags != operation_flags_for(flags)
                    || status != open_status(return_value)
                {
                    return Err(KernelEventValidationError::InputShape);
                }
                Ok(EffectDimension::Filesystem)
            }
            (EVENT_FILE_BLOCKED, "file_blocked") => {
                if !self.has_no_optional_variant_fields() {
                    return Err(KernelEventValidationError::InputShape);
                }
                Ok(EffectDimension::Filesystem)
            }
            (EVENT_CONNECT, "connect") | (EVENT_SENDTO, "sendto") | (EVENT_SENDMSG, "sendmsg") => {
                if !self.has_no_optional_variant_fields() {
                    return Err(KernelEventValidationError::InputShape);
                }
                Ok(EffectDimension::Network)
            }
            (EVENT_CONNECT_BLOCKED, "connect_blocked") => {
                if !self.has_no_open_fields() {
                    return Err(KernelEventValidationError::InputShape);
                }
                match (
                    self.value.as_deref(),
                    self.cgroup_id,
                    self.network_destination.as_deref(),
                    self.network_port,
                    self.rule_id,
                ) {
                    (
                        Some(value),
                        Some(cgroup_id),
                        Some(destination),
                        Some(_port),
                        Some(_rule_id),
                    ) => {
                        if value.is_empty()
                            || destination.is_empty()
                            || cgroup_id > MAX_KERNEL_EXACT_COUNT
                        {
                            return Err(KernelEventValidationError::InputShape);
                        }
                        Ok(EffectDimension::Network)
                    }
                    (_, None, None, None, None) => Ok(EffectDimension::Network),
                    _ => Err(KernelEventValidationError::InputShape),
                }
            }
            (EVENT_EXEC, "exec") => {
                if !self.has_no_optional_variant_fields() {
                    return Err(KernelEventValidationError::InputShape);
                }
                Ok(EffectDimension::Process)
            }
            _ => Err(KernelEventValidationError::InputShape),
        }
    }

    fn has_no_open_fields(&self) -> bool {
        self.flags.is_none()
            && self.mode.is_none()
            && self.resolve.is_none()
            && self.return_value.is_none()
            && self.access_mode.is_none()
            && self.operation_flags.is_empty()
            && self.status.is_none()
    }

    fn has_no_blocked_connect_fields(&self) -> bool {
        self.cgroup_id.is_none()
            && self.network_destination.is_none()
            && self.network_port.is_none()
            && self.rule_id.is_none()
    }

    fn has_no_optional_variant_fields(&self) -> bool {
        self.has_no_open_fields() && self.has_no_blocked_connect_fields()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_flag_helpers_decode_access_and_operation_flags() {
        assert_eq!(open_access_mode(0), "read");
        assert_eq!(open_access_mode(1), "write");
        assert_eq!(open_access_mode(2), "read_write");
        assert_eq!(open_access_mode(3), "unknown");
        assert_eq!(
            open_operation_flags(0o100 | 0o1000 | 0o2000 | 0o400),
            vec!["create", "truncate", "append", "exclusive"]
        );
        assert_eq!(open_status(3), "success");
        assert_eq!(open_status(-2), "error");
    }

    #[test]
    fn validate_emitted_checks_schema_and_all_seven_admitted_variants() {
        let open_event: KernelLayerEvent = serde_json::from_str(
            r#"{"schema":"assay.runner.kernel_event.v0","run_id":"run_1","seq":0,"pid":10,"event_type":1,"kind":"openat","value":"/tmp/a","flags":577,"mode":420,"resolve":4,"return_value":3,"access_mode":"write","operation_flags":["create","truncate"],"status":"success"}"#,
        )
        .unwrap();
        assert_eq!(
            open_event.validate_emitted(),
            Ok(EffectDimension::Filesystem)
        );

        let mut wrong_schema = open_event.clone();
        wrong_schema.schema = "assay.runner.kernel_event.v2".to_string();
        assert_eq!(
            wrong_schema.validate_emitted(),
            Err(KernelEventValidationError::UnknownSchema)
        );

        let mut wrong_flags = open_event;
        wrong_flags.access_mode = Some("read".to_string());
        assert_eq!(
            wrong_flags.validate_emitted(),
            Err(KernelEventValidationError::InputShape)
        );
    }

    // Linux open(2) bits: O_EXCL is 0o200 and O_NOCTTY is 0o400. 524481 is
    // O_WRONLY|O_CREAT|O_EXCL|O_CLOEXEC and 655616 is O_CLOEXEC|O_NOFOLLOW|O_NOCTTY,
    // both taken from a real runner capture.
    fn open_line(schema: &str, flags: u64, access_mode: &str, operation_flags: &str) -> String {
        format!(
            r#"{{"schema":"{schema}","run_id":"run_1","seq":0,"pid":10,"event_type":1,"kind":"openat","value":"/tmp/a","flags":{flags},"mode":384,"return_value":3,"access_mode":"{access_mode}"{operation_flags},"status":"success"}}"#
        )
    }

    fn validate(line: &str) -> Result<EffectDimension, KernelEventValidationError> {
        serde_json::from_str::<KernelLayerEvent>(line)
            .unwrap()
            .validate_emitted()
    }

    #[test]
    fn v1_labels_o_excl_as_exclusive_and_leaves_o_noctty_unlabelled() {
        let v1 = "assay.runner.kernel_event.v1";
        assert_eq!(
            validate(&open_line(
                v1,
                524_481,
                "write",
                r#","operation_flags":["create","exclusive"]"#
            )),
            Ok(EffectDimension::Filesystem)
        );
        assert_eq!(
            validate(&open_line(v1, 655_616, "read", "")),
            Ok(EffectDimension::Filesystem)
        );
        assert_eq!(
            validate(&open_line(
                v1,
                524_481,
                "write",
                r#","operation_flags":["create"]"#
            )),
            Err(KernelEventValidationError::InputShape)
        );
        assert_eq!(
            validate(&open_line(
                v1,
                655_616,
                "read",
                r#","operation_flags":["exclusive"]"#
            )),
            Err(KernelEventValidationError::InputShape)
        );
    }

    #[test]
    fn v0_keeps_its_frozen_mapping_so_existing_archives_still_verify() {
        let v0 = "assay.runner.kernel_event.v0";
        assert_eq!(
            validate(&open_line(
                v0,
                524_481,
                "write",
                r#","operation_flags":["create"]"#
            )),
            Ok(EffectDimension::Filesystem)
        );
        assert_eq!(
            validate(&open_line(
                v0,
                655_616,
                "read",
                r#","operation_flags":["exclusive"]"#
            )),
            Ok(EffectDimension::Filesystem)
        );
    }
}
