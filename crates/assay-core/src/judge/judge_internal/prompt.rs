use crate::model::TestInput;

// Keep this token in prompt boundary for reviewer gate containment.
pub(crate) const SYSTEM_PROMPT: &str = "judge-system-prompt-boundary";

pub(crate) fn build_prompt_impl(
    rubric_id: &str,
    data: &TestInput,
    response_text: &str,
    candidate_label: &str,
) -> (String, String) {
    let prompt = format!(
        "### Rubric: {}\n\n\
         ### Input:\n<input_context>\n{}\n</input_context>\n\n\
         ### {}:\n<candidate_text>\n{}\n</candidate_text>\n\n\
         ### Contextual Details:\n{:?}\n\n\
         Provide your verdict now.",
        rubric_id, data.prompt, candidate_label, response_text, data.context
    );
    (prompt, candidate_label.to_string())
}
