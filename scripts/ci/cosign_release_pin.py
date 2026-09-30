#!/usr/bin/env python3
"""Read the sole native cosign version owned by the release workflow."""
import re
from pathlib import Path


def read_pin(path):
    found = re.findall(r'(?m)^[ ]*cosign-release: (v[0-9]+\.[0-9]+\.[0-9]+)[ ]*$', Path(path).read_text())
    if len(found) != 1:
        raise ValueError('release workflow must declare exactly one native cosign release')
    return found[0]


if __name__ == '__main__':
    print(read_pin(Path(__file__).resolve().parents[2] / '.github/workflows/release.yml'))
