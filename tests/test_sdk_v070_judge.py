import json
import os
from unittest.mock import MagicMock, patch

import pytest
from verdict_sdk.evaluator import Evaluator
from verdict_sdk.judge.openai_judge import OpenAIJudge
from verdict_sdk.judge.types import JudgeRequest

TRACE_CONTENT = '{"kind": "model", "content": "Paris is the capital of France."}'


@pytest.fixture
def judge_setup(tmp_path):
    cwd = os.getcwd()
    os.chdir(tmp_path)
    yield tmp_path
    os.chdir(cwd)


def test_judge_execution_mocked(judge_setup):
    """
    Verifies that Evaluator calls OpenAIJudge (Production Wiring),
    which uses the Mocked OpenAI client, and returns parsed score.
    """
    import sys

    sys.stderr.write(f"DEBUG EVAL SOURCE: {Evaluator.__module__}\\n")

    tmp = judge_setup
    (tmp / "trace.jsonl").write_text(TRACE_CONTENT)
    (tmp / "eval.yaml").write_text(
        """
version: 1
judge:
  model: gpt-4o
  cache: true
tests:
  - id: t1
    prompt: "Capital?"
    metrics:
      - name: faithfulness
        kind: judge
        threshold: 0.8
"""
    )
    # Mocking openai package
    # We patch where it is imported:
    with patch("verdict_sdk.judge.openai_judge.OpenAI") as mock_cls:
        # Mock Client
        mock_client = MagicMock()
        mock_cls.return_value = mock_client

        # Mock Completion
        mock_completion = MagicMock()
        mock_completion.choices[0].message.content = (
            '{"score": 0.95, "rationale": "Perfect", "passed": true}'
        )
        mock_client.chat.completions.create.return_value = mock_completion

        # Mocking specific check
        # We need to ensure logic for "always_true" calls our mock
        # "always_true" is not builtin. Use 'regex_match' or similar mocked by ev?
        # Actually Evaluator uses generic prompt judge if config specifies.

        # Let's verify OpenAIJudge directly for unit testing logic
        judge = OpenAIJudge(client=mock_client)

        # 1. Standard Case
        mock_completion.choices[0].message.content = (
            '{"score": 0.95, "rationale": "Perfect", "passed": true}'
        )
        # Metric object can be mock, but name must be string if used in logging/JSON?
        # OpenAIJudge constructs user message: {"metric": req.metric.name, ...}
        # If metric is MagicMock, metric.name is MagicMock object unless configured.
        metric_mock = MagicMock()
        metric_mock.name = "test_metric"
        req = JudgeRequest(metric=metric_mock, question="q", answer="a", context="c")

        res = judge.evaluate(req)
        assert res.score == 0.95
        assert res.passed == True
        assert res.rationale == "Perfect"

        # 2. Reason Alias Case
        mock_completion.choices[0].message.content = (
            '{"score": 0.8, "reason": "Good enough", "passed": true}'
        )
        res = judge.evaluate(req)
        assert res.score == 0.8
        assert res.rationale == "Good enough"
        assert res.passed == True

        # 3. Invalid JSON (Snippet Check)
        mock_completion.choices[0].message.content = "NOT JSON HERE"
        from verdict_sdk.errors import JudgeParseError

        with pytest.raises(JudgeParseError) as exc:
            judge.evaluate(req)
        assert "NOT JSON HERE" in exc.value.snippet

        # 4. Missing Score
        mock_completion.choices[0].message.content = (
            '{"rationale": "No score", "passed": false}'
        )
        with pytest.raises(JudgeParseError) as exc:
            judge.evaluate(req)
        assert "Missing 'score'" in str(exc.value)

        # 5. Invalid Score Type
        mock_completion.choices[0].message.content = (
            '{"score": "not_a_number", "passed": false}'
        )
        with pytest.raises(JudgeParseError) as exc:
            judge.evaluate(req)
        assert "Invalid score value" in str(exc.value)

        # Reset Mock for Evaluator Run
        mock_completion.choices[0].message.content = (
            '{"score": 0.95, "rationale": "Perfect", "passed": true}'
        )

        # Edge Case 6: Empty JSON
        mock_completion.choices[0].message.content = "{}"
        with pytest.raises(JudgeParseError) as exc:
            judge.evaluate(req)
        assert "Missing 'score'" in str(exc.value)

        # Edge Case 7: Null Score
        mock_completion.choices[0].message.content = (
            '{"score": null, "rationale": "Null score", "passed": false}'
        )
        with pytest.raises(JudgeParseError) as exc:
            judge.evaluate(req)
        assert "Invalid score value" in str(exc.value)

        # Edge Case 8: Score Clamping
        # > 1.0 -> 1.0
        mock_completion.choices[0].message.content = (
            '{"score": 1.5, "rationale": "Too good", "passed": true}'
        )
        res = judge.evaluate(req)
        assert res.score == 1.0

        # < 0.0 -> 0.0
        mock_completion.choices[0].message.content = (
            '{"score": -0.5, "rationale": "Too bad", "passed": false}'
        )
        res = judge.evaluate(req)
        assert res.score == 0.0

        # Edge Case 9: Unicode/Emoji
        mock_completion.choices[0].message.content = (
            '{"score": 1.0, "rationale": "🚀 Great job! ✨", "passed": true}'
        )
        res = judge.evaluate(req)
        assert res.rationale == "🚀 Great job! ✨"

        # Edge Case 10: Long Malformed Snippet
        long_junk = "X" * 500
        mock_completion.choices[0].message.content = long_junk
        with pytest.raises(JudgeParseError) as exc:
            judge.evaluate(req)
        assert len(exc.value.snippet) <= 200
        assert exc.value.snippet == ("X" * 200)

        # Edge Case 11: Valid JSON but not an object (list)
        mock_completion.choices[0].message.content = '[{"score": 1.0}]'
        with pytest.raises(JudgeParseError) as exc:
            judge.evaluate(req)
        assert "Response is not a JSON object" in str(exc.value)

        # Reset Mock for Evaluator Run
        mock_completion.choices[0].message.content = (
            '{"score": 0.95, "rationale": "Perfect", "passed": true}'
        )

        ev = Evaluator()
        run = ev.run("trace.jsonl")

        assert run.passed is True
        m = run.tests[0].metrics[0]
        assert m.name == "faithfulness"
        assert m.value == 0.95
        # Threshold 0.8, Value 0.95 -> Passed
        assert m.passed is True
        assert m.meta["rationale"] == "Perfect"

        # Verify call arguments (context empty)
        call_args = mock_client.chat.completions.create.call_args
        assert call_args is not None
        # Check messages json content
        # messages=[{...}, {role: user, content: JSON string}]
        msgs = call_args.kwargs["messages"]
        user_content = json.loads(msgs[1]["content"])
        assert user_content["context"] == ""

        # Setup Cache Hit
        # To test cache, we must ensure Evaluator uses a CachedJudge.
        # ev.judge is initialized with CachedJudge if cache: true in config.
        # But for cache HIT, we need persistence.
        # CachedJudge writes to disk.
        # Run 1 above should have written to cache.

        mock_client.chat.completions.create.reset_mock()
        ev2 = Evaluator()
        run2 = ev2.run("trace.jsonl")

        assert run2.tests[0].metrics[0].value == 0.95
        mock_client.chat.completions.create.assert_not_called()
