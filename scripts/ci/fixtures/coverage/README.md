# Coverage fixtures

- `input_basic.jsonl`: basic positive fixture with declared tools, routes, and one unknown tool.
- `input_tool_name_fallback.jsonl`: positive fixture for `tool_name` fallback parsing.
- `input_missing_tool_fields.jsonl`: negative fixture for missing or empty `tool` / `tool_name` contract failures.
- `decision_event_basic.jsonl`: decision-event fixture for B3.2 wrap-path normalization from nested `data.tool` and `data.tool_classes`.
