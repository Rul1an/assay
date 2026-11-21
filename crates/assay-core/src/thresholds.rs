use serde::Serialize;

assay_common::closed! {
    #[derive(Default, Serialize)]
    pub struct ThresholdConfig {
        #[omit_none]
        min_score: Option<f64>,
        #[omit_none]
        block_on_warn: Option<bool>,
    }
}
