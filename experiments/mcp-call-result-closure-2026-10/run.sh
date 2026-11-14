#!/bin/sh
set -eu
# Trusted synthetic macOS reproduction. No live tool or model calls.
# Arguments: Assay repository, pinned CA checkout, NEW build dir, NEW result dir.
if [ "$#" -ne 4 ]; then
    echo "usage: $0 ASSAY_REPO CA_REPO NEW_BUILD_DIR NEW_RESULT_DIR" >&2
    exit 2
fi
probe="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
python3 "$probe/build_parser.py" "$1" "$3"
export CLOSURE_PARSER="$3/target/debug/closure-parser-probe"
CLOSURE_PARSER_SHA256="$(shasum -a 256 "$CLOSURE_PARSER" | cut -d ' ' -f1)"
export CLOSURE_PARSER_SHA256
export PYTHONDONTWRITEBYTECODE=1
python3 -m unittest discover -s "$probe" -p 'test_*.py' -v
python3 "$probe/run_probe.py" "$1" "$2" "$3" "$4"
python3 "$probe/independent_reproducer.py" "$4/result.json"
