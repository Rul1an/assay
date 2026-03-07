use assay_adapter_api::{
    AdapterError, AdapterErrorKind, AdapterResult, AttachmentWriter, ConvertOptions, RawPayloadRef,
};

pub(crate) fn write_raw_payload_ref(
    payload: &[u8],
    media_type: &str,
    options: &ConvertOptions,
    attachments: &dyn AttachmentWriter,
) -> AdapterResult<RawPayloadRef> {
    if let Some(limit) = options.max_payload_bytes {
        if payload.len() as u64 > limit {
            return Err(AdapterError::new(
                AdapterErrorKind::Measurement,
                format!("payload exceeds max_payload_bytes ({})", limit),
            ));
        }
    }

    attachments.write_raw_payload(payload, media_type)
}
