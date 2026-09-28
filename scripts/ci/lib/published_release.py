"""Shared latest-stable-release metadata validation; no network or authentication."""
import json
from pathlib import Path
import re

MAX_METADATA_BYTES = 1048576


def load_metadata(path):
    path = Path(path)
    with path.open('rb') as source:
        raw = source.read(MAX_METADATA_BYTES + 1)
    if len(raw) > MAX_METADATA_BYTES:
        raise ValueError('latest published release metadata exceeds 1048576-byte limit')
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f'duplicate metadata key: {key}')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique)


def version(tag):
    return tuple(map(int, tag.removeprefix('v').split('.')))


def published_tag(release, pin=None):
    if not isinstance(release, dict):
        raise ValueError('latest published release metadata must be an object')
    latest = release.get('tag_name')
    if not isinstance(latest, str) or not re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+', latest):
        raise ValueError(f'latest published release has an invalid stable tag: {latest!r}')
    if release.get('draft') is not False or release.get('prerelease') is not False:
        raise ValueError(f'latest published release {latest} is draft or prerelease')
    if pin is not None and pin != latest:
        if version(pin) == version(latest):
            raise ValueError(f'install pin {pin} does not exactly match latest published release {latest}')
        relation = 'leads' if version(pin) > version(latest) else 'trails'
        raise ValueError(f'install pin {pin} {relation} latest published release {latest}')
    assets = release.get('assets')
    names = [a.get('name') for a in assets if isinstance(a, dict)] if isinstance(assets, list) else []
    archive = f'assay-{latest}-x86_64-unknown-linux-gnu.tar.gz'
    for name in (archive, archive + '.sha256'):
        if name not in names:
            raise ValueError(f'latest published release {latest} lacks {name}')
    return latest
