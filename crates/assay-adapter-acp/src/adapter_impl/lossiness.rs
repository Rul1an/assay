use assay_adapter_api::LossinessLevel;

pub(crate) fn classify_lossiness(unmapped_fields_count: u32) -> LossinessLevel {
    if unmapped_fields_count == 0 {
        LossinessLevel::None
    } else if unmapped_fields_count <= 2 {
        LossinessLevel::Low
    } else {
        LossinessLevel::High
    }
}
