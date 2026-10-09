"""Hostile local inputs must refuse before unrestricted reads or JSON decoding."""
import gzip
import tempfile
import unittest
from pathlib import Path

try:
    from evidence_io import Limits, Refused, Store, loads
except ImportError:
    Limits = Refused = Store = loads = None
try:
    from evidence_io import ArchiveStore, pack, unpack
except ImportError:
    ArchiveStore = pack = unpack = None


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


def header(name, size=0, typeflag=b'0', prefix=b'', magic=b'ustar\x0000', checksum=None):
    block = bytearray(512)
    block[0:len(name)] = name
    block[100:108] = b'0000644\x00'
    block[108:116] = block[116:124] = b'0000000\x00'
    block[124:136] = b'%011o\x00' % size
    block[136:148] = b'00000000000\x00'
    block[156:157] = typeflag
    block[257:265] = magic
    block[345:345 + len(prefix)] = prefix
    block[148:156] = b' ' * 8
    total = sum(block) if checksum is None else checksum
    block[148:156] = b'%06o\x00 ' % total
    return bytes(block)


def member(name, data=b'', **kwargs):
    return header(name, len(data), **kwargs) + data + b'\x00' * (-len(data) % 512)


END = b'\x00' * 1024


class BoundedArchive(unittest.TestCase):
    """One committed archive stands in for a large retained tree; it is unpacked, never trusted."""

    def setUp(self):
        self.assertIsNotNone(ArchiveStore, 'bounded archive reader not implemented')
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def write(self, raw, name='a.tar.gz'):
        path = self.root / name
        with gzip.GzipFile(path, 'wb', mtime=0) as stream:
            stream.write(raw)
        return path

    def refused(self, raw, reason, limits=None):
        # unpack() directly, so a later layer (O_EXCL, Store's own re-scan) cannot
        # stand in for the guard named by `reason`; and nothing may be left behind.
        path = raw if isinstance(raw, Path) else self.write(raw)
        dest = self.root / 'out'
        with self.assertRaisesRegex(Refused, reason):
            unpack(path, dest, *([limits] if limits else []))
        self.assertFalse(dest.exists())

    def test_round_trip_is_byte_exact_and_deterministic(self):
        src = self.root / 'src'
        (src / 'run/deep').mkdir(parents=True)
        long_dir = 'd' * 90
        (src / long_dir).mkdir()
        files = {'inventory.json': b'{}', 'run/deep/blob': bytes(range(256)) * 3,
                 'run/empty': b'', long_dir + '/' + 'f' * 40: b'long path'}
        for name, data in files.items():
            (src / name).write_bytes(data)
        pack(src, self.root / 'one.tar.gz')
        pack(src, self.root / 'two.tar.gz')
        self.assertEqual((self.root / 'one.tar.gz').read_bytes(), (self.root / 'two.tar.gz').read_bytes())
        with ArchiveStore(self.root / 'one.tar.gz') as store:
            self.assertEqual(set(store.names), set(files))
            for name, data in files.items():
                self.assertEqual(store.read(name), data)
        dest = self.root / 'out'
        unpack(self.root / 'one.tar.gz', dest)
        self.assertEqual({p.relative_to(dest).as_posix() for p in dest.rglob('*') if p.is_file()}, set(files))

    def test_minimal_member_accepted(self):
        path = self.write(member(b'a', b'{"ok":true}') + END)
        with ArchiveStore(path) as store:
            self.assertEqual(store.json('a'), {'ok': True})

    def test_unpacked_tree_is_removed_after_use(self):
        path = self.write(member(b'a', b'x') + END)
        with ArchiveStore(path) as store:
            unpacked = Path(store.root)
            self.assertTrue(unpacked.is_dir())
        self.assertFalse(unpacked.exists())

    def test_links_devices_and_directory_members_refused(self):
        for flag in [b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'x', b'g', b'L', b'K']:
            with self.subTest(flag=flag):
                self.refused(member(b'a', typeflag=flag) + END, 'regular file member required')

    def test_unsafe_names_refused(self):
        for name, prefix in [(b'../a', b''), (b'/a', b''), (b'./a', b''), (b'x//a', b''),
                             (b'x/../a', b''), (b'x\\a', b''), (b'a', b'../x'), (b'a', b'/x'), (b'', b'')]:
            with self.subTest(name=name, prefix=prefix):
                self.refused(member(name, b'x', prefix=prefix) + END, 'relative path required|path length')
        deep = member(b'/'.join([b'd'] * 40), b'x', prefix=b'/'.join([b'd'] * 25)) + END
        self.refused(deep, 'directory depth limit')

    def test_duplicate_and_file_directory_conflicts_refused(self):
        self.refused(member(b'a', b'1') + member(b'a', b'2') + END, 'duplicate or overlapping member')
        self.refused(member(b'a', b'1') + member(b'a/b', b'2') + END, 'member below a file member')
        self.refused(member(b'a/b', b'1') + member(b'a', b'2') + END, 'duplicate or overlapping member')

    def test_malformed_headers_and_framing_refused(self):
        self.refused(header(b'a', 1, checksum=1) + b'x'.ljust(512, b'\x00') + END, 'header checksum')
        self.refused(member(b'a', b'x', magic=b'ustar  \x00') + END, 'ustar header required')
        self.refused(member(b'a', b'x'), 'truncated archive')
        self.refused(member(b'a', b'x') + b'\x00' * 512, 'truncated archive')
        self.refused(header(b'a', 600) + b'x' * 512 + END, 'truncated archive')
        self.refused(member(b'a', b'x') + END + member(b'b', b'y'), 'data after end of archive')
        self.refused(b'not a tar', 'truncated archive')
        self.refused(b'', 'truncated archive')
        self.refused(END, 'empty archive')

    def test_size_count_and_expansion_bounds(self):
        self.refused(member(b'a', b'x' * 17) + END, 'file byte limit', Limits(file_bytes=16))
        self.refused(member(b'a', b'x' * 9) + member(b'b', b'x' * 9) + END, 'total byte limit',
                     Limits(total_bytes=16))
        self.refused(member(b'a', b'x') + member(b'b', b'y') + END, 'entry count limit', Limits(files=1))
        self.refused(member(b'd/a', b'x') + END, 'entry count limit', Limits(files=1))
        # Trailing zero padding is legal framing, but not without bound.
        self.refused(member(b'a', b'x') + END + b'\x00' * (64 * 512), 'decompressed size limit',
                     Limits(files=1, total_bytes=512))

    def test_archive_path_must_be_a_plain_regular_file(self):
        good = self.write(member(b'a', b'x') + END)
        (self.root / 'link.tar.gz').symlink_to(good)
        (self.root / 'dir.tar.gz').mkdir()
        (self.root / 'plain.tar.gz').write_bytes(member(b'a', b'x') + END)
        for name, reason in [('link.tar.gz', 'archive refused'), ('dir.tar.gz', 'archive must be a regular file'),
                             ('missing.tar.gz', 'archive refused'), ('plain.tar.gz', 'archive refused')]:
            with self.subTest(name=name):
                self.refused(self.root / name, reason)
                with self.assertRaises(Refused), ArchiveStore(self.root / name):
                    pass

    def test_corrupt_truncated_and_trailing_compression_refused(self):
        raw = self.write(member(b'a', b'x' * 2000) + END).read_bytes()
        cases = {'truncated': (raw[:len(raw) // 2], 'truncated'),
                 'no-trailer': (raw[:-8], 'truncated compressed stream'),
                 'crc': (raw[:-8] + bytes([raw[-8] ^ 1]) + raw[-7:], 'archive refused'),
                 'trailing': (raw + b'junk', 'data after the compressed stream'),
                 'second-member': (raw + raw, 'data after the compressed stream')}
        for mode, (data, reason) in cases.items():
            with self.subTest(mode=mode):
                path = self.root / (mode + '.tar.gz')
                path.write_bytes(data)
                self.refused(path, reason)

    def test_expansion_is_refused_before_full_materialization(self):
        # 64 MiB of zeros compresses to ~64 KiB, under the archive byte limit for these
        # limits; the expansion limit (~83 KiB) must trip on the stream, long before 64 MiB.
        path = self.root / 'bomb.tar.gz'
        with gzip.GzipFile(path, 'wb', mtime=0) as stream:
            stream.write(member(b'a', b'x') + END)
            for _ in range(64):
                stream.write(b'\x00' * (1024 * 1024))
        self.refused(path, 'decompressed size limit', Limits(files=4, total_bytes=64 * 1024))

    def test_failed_unpack_leaves_nothing_behind(self):
        scratch = self.root / 'tmp'
        scratch.mkdir()
        old = tempfile.tempdir
        tempfile.tempdir = str(scratch)
        self.addCleanup(setattr, tempfile, 'tempdir', old)
        bad = self.write(member(b'a', b'x') + member(b'../b', b'y') + END)
        with self.assertRaises(Refused):
            with ArchiveStore(bad):
                pass
        self.assertEqual(list(scratch.iterdir()), [])
        dest = self.root / 'partial'
        with self.assertRaises(Refused):
            unpack(bad, dest)
        self.assertFalse(dest.exists())

    def test_oversized_archive_refused_before_decompression(self):
        limits = Limits(files=1, total_bytes=16)
        path = self.write(member(b'a', b'x') + END)
        with open(path, 'ab') as stream:
            stream.write(b'\x00' * 32 * 1024)
        self.refused(path, 'archive byte limit', limits)

    def test_unpack_refuses_existing_destination(self):
        path = self.write(member(b'a', b'x') + END)
        (self.root / 'taken').mkdir()
        with self.assertRaises(Refused):
            unpack(path, self.root / 'taken')


if __name__ == '__main__':
    unittest.main()
