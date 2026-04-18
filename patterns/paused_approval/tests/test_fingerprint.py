from __future__ import annotations

import unittest

from patterns.paused_approval.fingerprint import derive_resume_state_ref


class DeriveResumeStateRefTests(unittest.TestCase):
    def test_same_mapping_content_same_hash(self) -> None:
        left = {"b": 1, "a": [True, "x"]}
        right = {"a": [True, "x"], "b": 1}
        self.assertEqual(derive_resume_state_ref(left), derive_resume_state_ref(right))

    def test_string_and_bytes_match(self) -> None:
        self.assertEqual(
            derive_resume_state_ref("serialized-state"),
            derive_resume_state_ref(b"serialized-state"),
        )

    def test_integer_float_normalizes(self) -> None:
        self.assertEqual(derive_resume_state_ref({"x": 1.0}), derive_resume_state_ref({"x": 1}))

    def test_non_integer_float_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "non-integer floats"):
            derive_resume_state_ref({"x": 1.25})

    def test_non_finite_float_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "non-finite floats"):
            derive_resume_state_ref({"x": float("inf")})

    def test_list_payload_supported(self) -> None:
        result = derive_resume_state_ref([{"call_id": "one"}, {"call_id": "two"}])
        self.assertTrue(result.startswith("runstate:sha256:"))

    def test_nested_payload_supported(self) -> None:
        result = derive_resume_state_ref({"pause": {"reason": "tool_approval"}})
        self.assertTrue(result.startswith("runstate:sha256:"))


if __name__ == "__main__":
    unittest.main()
