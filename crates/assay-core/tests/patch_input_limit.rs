use assay_core::fix::{apply_ops_to_file, apply_ops_to_text};
use assay_core::model::MAX_POLICY_INPUT_BYTES;

#[test]
fn patch_targets_accept_the_limit_and_refuse_one_more_byte_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    for (extension, json) in [("json", true), ("yaml", false)] {
        let path = dir.path().join(format!("target.{extension}"));
        let mut input = "{}".to_owned();
        input.push_str(&" ".repeat(MAX_POLICY_INPUT_BYTES as usize - input.len()));
        std::fs::write(&path, &input).unwrap();
        assert!(apply_ops_to_text(&input, &[], json).is_ok());
        apply_ops_to_file(&path, &[]).unwrap();

        input.push(' ');
        std::fs::write(&path, &input).unwrap();
        let error = apply_ops_to_text(&input, &[], json).unwrap_err();
        assert!(format!("{error:#}").contains(&MAX_POLICY_INPUT_BYTES.to_string()));
        let error = apply_ops_to_file(&path, &[]).unwrap_err();
        assert!(format!("{error:#}").contains(&MAX_POLICY_INPUT_BYTES.to_string()));
        assert_eq!(std::fs::read(&path).unwrap(), input.as_bytes());
    }
}
