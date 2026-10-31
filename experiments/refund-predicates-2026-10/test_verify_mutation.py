"""Fixed-run offline evidence tests, literal outcomes and adversarial copies."""
import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from evidence_io import Refused

try:
    from evidence_io import pack, unpack
except ImportError:
    pack = unpack = None

try:
    from verify_mutation import check_receipt, compare_claims, verify
except ImportError:
    check_receipt = compare_claims = verify = None

HERE = Path(__file__).resolve().parent
RETAINED = HERE / 'record/mutation/retained.tar.gz'


class MutationEvidence(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(verify, 'offline verifier not implemented')

    def unpacked(self):
        self.assertIsNotNone(unpack, 'bounded archive reader not implemented')
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        target = Path(directory.name).resolve() / 'retained'
        unpack(RETAINED, target)
        return target

    def test_full_original_run(self):
        result = verify(RETAINED, HERE / 'record/mutation', HERE / 'record/c3')
        self.assertEqual(result['receipts'], 900)
        self.assertEqual(result['dispatches'], 900)
        self.assertEqual(result['inventory_files'], 7098)
        self.assertEqual(result['blob_files'], 3961)
        self.assertEqual(result['target_counts'], {'F1': 5, 'F2': 4, 'F3': 6, 'F4a': 1, 'F4b': 1, 'F5': 3, 'F6': 3})
        self.assertEqual(result['unexpected_axes'], 0)
        self.assertEqual(result['result_sha256'], 'c26f5b562e2963a469540237591ad3f3ca24e8a1ef0027164efbe50f536b9625')
        self.assertEqual(result['execution'], 'not-performed')

    def test_directory_route_gives_the_same_record(self):
        self.assertEqual(verify(self.unpacked(), HERE / 'record/mutation', HERE / 'record/c3'),
                         verify(RETAINED, HERE / 'record/mutation', HERE / 'record/c3'))

    def test_tampered_archive_refused(self):
        target = self.unpacked()
        receipt = next(target.rglob('receipt-*.json'))
        receipt.write_bytes(receipt.read_bytes().replace(b'"returncode": 0', b'"returncode": 1'))
        tampered = target.parent / 'tampered.tar.gz'
        pack(target, tampered)
        with self.assertRaises(Refused):
            verify(tampered, HERE / 'record/mutation', HERE / 'record/c3')

    def test_cli_default_reads_the_committed_archive(self):
        run = subprocess.run([sys.executable, '-B', str(HERE / 'verify_mutation.py')],
                             capture_output=True, check=False)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertEqual(json.loads(run.stdout)['result_sha256'],
                         'c26f5b562e2963a469540237591ad3f3ca24e8a1ef0027164efbe50f536b9625')

    def test_tampered_missing_extra_and_symlink_receipts_refused(self):
        target = self.unpacked()
        receipt = next(target.rglob('receipt-*.json'))
        original = receipt.read_bytes()
        for mode in ['corrupt', 'missing', 'symlink']:
            with self.subTest(mode=mode):
                if receipt.exists() or receipt.is_symlink():
                    receipt.unlink()
                if mode == 'corrupt':
                    receipt.write_bytes(original.replace(b'"returncode": 0', b'"returncode": 1'))
                elif mode == 'symlink':
                    receipt.symlink_to('/dev/zero')
                with self.assertRaises(Refused):
                    verify(target, HERE / 'record/mutation', HERE / 'record/c3')
        receipt.unlink()
        receipt.write_bytes(original)
        (target / 'extra').write_bytes(b'x')
        with self.assertRaises(Refused):
            verify(target, HERE / 'record/mutation', HERE / 'record/c3')

    def test_process_success_and_identity_are_required(self):
        receipt = json.loads(next(self.unpacked().rglob('receipt-*.json')).read_bytes())
        check_receipt(receipt)
        for key, value in [('returncode', True), ('returncode', 1), ('abnormal', 'timeout'),
                           ('stderr_size', 1), ('selector_presence', [True, False]),
                           ('stdout_size', -1), ('source_sha256', '../outside')]:
            bad = dict(receipt, **{key: value})
            with self.subTest(key=key), self.assertRaises(Refused):
                check_receipt(bad)

    def test_projection_keeps_axis_changes_and_extras_separate(self):
        oracle = {'X': [{'report_id': 'r', 'report_seq': 1, 'reported_outcome': 'unknown',
                        'conflict': True, 'eligible_delivery_ids': [], 'eligible_result_ids': [],
                        'definite_support': {'applicable': False, 'result': None, 'reason': 'not_definite_report'},
                        'uncertainty_calibration': {'applicable': True, 'result': 'ESTABLISHED', 'reason': 'conflicting_terminal_evidence'}}]}
        got = copy.deepcopy(oracle)
        got['X'][0]['conflict'] = False
        got['X'][0]['uncertainty_calibration']['reason'] = 'unnecessarily_unknown'
        changed, axes, extras = compare_claims(got, oracle)
        self.assertEqual(changed, ['X'])
        self.assertEqual(axes, [])
        self.assertEqual({x['field'] for x in extras}, {'conflict', 'uncertainty_calibration.reason'})
        got['X'][0]['uncertainty_calibration']['result'] = 'CONTRADICTED'
        _, axes, _ = compare_claims(got, oracle)
        self.assertEqual(axes, [{'case_id': 'X', 'report_id': 'r', 'axis': 'uncertainty_calibration',
                                'oracle': [True, 'ESTABLISHED'], 'observed': [True, 'CONTRADICTED']}])
        with self.assertRaises(Refused):
            compare_claims({}, oracle)

    def test_cli_missing_evidence_never_partial_pass(self):
        run = subprocess.run([sys.executable, '-B', str(HERE / 'verify_mutation.py'), '--retained',
                              str(HERE / 'missing')], capture_output=True, check=False)
        self.assertEqual(run.returncode, 2)
        self.assertEqual(run.stdout, b'')
        self.assertTrue(run.stderr.startswith(b'refused:'), run.stderr)


if __name__ == '__main__':
    unittest.main()
