"""Hostile local inputs must refuse before unrestricted reads or JSON decoding."""
import tempfile
import unittest
from pathlib import Path

try:
    from evidence_io import Limits, Refused, Store, loads
except ImportError:
    Limits = Refused = Store = loads = None


class BoundedIO(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(Store, 'bounded evidence loader not implemented')
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def test_plain_file_and_duplicate_keys(self):
        (self.root / 'a').write_bytes(b'{"ok":true}')
        with Store(self.root) as store:
            self.assertEqual(store.json('a'), {'ok': True})
        with self.assertRaises(Refused):
            loads(b'{"a":0,"\\u0061":1}')

    def test_depth_string_brackets_and_token_limit(self):
        self.assertEqual(loads(b'[[0]]', Limits(depth=2)), [[0]])
        self.assertEqual(loads(b'"[[[[[["', Limits(depth=1)), '[[[[[[')
        for data in [b'[[[0]]]', b'{"x":NaN}', b'{}{}', b'\xef\xbb\xbf{}', b'\xff']:
            with self.subTest(data=data), self.assertRaises(Refused):
                loads(data, Limits(depth=2))
        with self.assertRaises(Refused):
            loads(b'[0,0,0,0]', Limits(tokens=3))

    def test_file_and_total_count_bounds(self):
        (self.root / 'a').write_bytes(b' ' * 17)
        for limits in [Limits(file_bytes=16), Limits(total_bytes=16), Limits(files=0)]:
            with self.subTest(limits=limits), self.assertRaises(Refused), Store(self.root, limits):
                pass

    def test_path_traversal_symlinks_and_archive_refused(self):
        (self.root / 'a').write_bytes(b'{}')
        with Store(self.root) as store:
            for name in ['../a', '/a', './a', 'x//a', 'x/../a', 'x\\a']:
                with self.subTest(name=name), self.assertRaises(Refused):
                    store.read(name)
        with self.assertRaises(Refused):
            Store(self.root / 'a')
        (self.root / 'link').symlink_to(self.root / 'a')
        with self.assertRaises(Refused):
            Store(self.root)

    def test_empty_directory_count(self):
        for index in range(3):
            (self.root / str(index)).mkdir()
        with self.assertRaises(Refused), Store(self.root, Limits(files=2)):
            pass

    def test_nonfinite_exponents(self):
        with self.assertRaises(Refused):
            loads(b'1e9999')

    def test_post_scan_replacement_refused(self):
        (self.root / 'a').write_bytes(b'{}')
        with Store(self.root) as store:
            (self.root / 'a').unlink()
            (self.root / 'a').symlink_to('/dev/zero')
            with self.assertRaises(Refused):
                store.read('a')


if __name__ == '__main__':
    unittest.main()
