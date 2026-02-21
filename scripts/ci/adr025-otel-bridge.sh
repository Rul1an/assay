#!/usr/bin/env bash
set -euo pipefail

IN=""
OUT_JSON=""
OUT_MD=""
ASSAY_VERSION="${ASSAY_VERSION:-0.0.0-script}"

usage() {
  echo "Usage: $0 --in <otel_input.json> --out-json <otel_bridge_report_v1.json> --out-md <otel_bridge_report_v1.md> [--assay-version <v>]"
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --in)
      IN="$2"
      shift 2
      ;;
    --out-json)
      OUT_JSON="$2"
      shift 2
      ;;
    --out-md)
      OUT_MD="$2"
      shift 2
      ;;
    --assay-version)
      ASSAY_VERSION="$2"
      shift 2
      ;;
    -h|--help)
      usage
      ;;
    *)
      echo "Unknown arg: $1"
      usage
      ;;
  esac
done

[[ -z "$IN" || -z "$OUT_JSON" || -z "$OUT_MD" ]] && usage
[[ -f "$IN" ]] || {
  echo "Measurement error: missing input file: $IN"
  exit 2
}

python3 - <<'PY' "$IN" "$OUT_JSON" "$OUT_MD" "$ASSAY_VERSION"
import json
import sys
from typing import Any, Dict, List

in_path, out_json, out_md, assay_version = sys.argv[1:]


def die(msg: str):
    print(f"Measurement error: {msg}")
    raise SystemExit(2)


def to_str_int(x: Any) -> str:
    if isinstance(x, int):
        return str(x)
    if isinstance(x, str) and x.isdigit():
        return x
    die(f"expected unix_nano int or digit-string, got {x!r}")


def norm_hex(x: Any, n: int, label: str) -> str:
    if not isinstance(x, str):
        die(f"{label} must be string")
    y = x.strip().lower()
    if len(y) != n or any(c not in "0123456789abcdef" for c in y):
        die(f"{label} must be lowercase hex len={n}, got {x!r}")
    return y


def kv_list(obj: Any) -> List[Dict[str, Any]]:
    if obj is None:
        return []
    if isinstance(obj, list):
        out = []
        for it in obj:
            if not isinstance(it, dict) or "key" not in it or "value" not in it:
                die("attributes list items must be {key,value}")
            out.append({"key": str(it["key"]), "value": it["value"]})
        out.sort(key=lambda x: x["key"])
        return out
    if isinstance(obj, dict):
        out = [{"key": str(k), "value": v} for k, v in obj.items()]
        out.sort(key=lambda x: x["key"])
        return out
    die("attributes must be object or array")


def event_list(obj: Any) -> List[Dict[str, Any]]:
    if obj is None:
        return []
    if not isinstance(obj, list):
        die("events must be array")
    out = []
    for e in obj:
        if not isinstance(e, dict):
            die("event must be object")
        name = e.get("name")
        t = e.get("time_unix_nano")
        attrs = kv_list(e.get("attributes", {}))
        if not isinstance(name, str) or not name:
            die("event.name required")
        out.append({"name": name, "time_unix_nano": to_str_int(t), "attributes": attrs})
    out.sort(key=lambda x: (x["time_unix_nano"], x["name"]))
    return out


def link_list(obj: Any) -> List[Dict[str, Any]]:
    if obj is None:
        return []
    if not isinstance(obj, list):
        die("links must be array")
    out = []
    for l in obj:
        if not isinstance(l, dict):
            die("link must be object")
        tid = norm_hex(l.get("trace_id"), 32, "link.trace_id")
        sid = norm_hex(l.get("span_id"), 16, "link.span_id")
        attrs = kv_list(l.get("attributes", {}))
        out.append({"trace_id": tid, "span_id": sid, "attributes": attrs})
    out.sort(key=lambda x: (x["trace_id"], x["span_id"]))
    return out


data = json.load(open(in_path, "r", encoding="utf-8"))
traces_in = data.get("traces")
if not isinstance(traces_in, list):
    die("top-level traces must be array")

traces_out = []
span_count = 0

for t in traces_in:
    if not isinstance(t, dict):
        die("trace must be object")
    trace_id = norm_hex(t.get("trace_id"), 32, "trace_id")
    spans_in = t.get("spans")
    if not isinstance(spans_in, list):
        die("trace.spans must be array")

    spans_out = []
    for s in spans_in:
        if not isinstance(s, dict):
            die("span must be object")
        span_id = norm_hex(s.get("span_id"), 16, "span_id")
        parent = s.get("parent_span_id")
        parent_id = norm_hex(parent, 16, "parent_span_id") if parent is not None else None

        name = s.get("name")
        kind = s.get("kind")
        if not isinstance(name, str) or not name:
            die("span.name required")
        if kind not in ("INTERNAL", "SERVER", "CLIENT", "PRODUCER", "CONSUMER"):
            die(f"span.kind invalid: {kind!r}")

        st = to_str_int(s.get("start_time_unix_nano"))
        et = to_str_int(s.get("end_time_unix_nano"))

        attrs = kv_list(s.get("attributes", {}))
        events = event_list(s.get("events", []))
        links = link_list(s.get("links", []))

        span_obj = {
            "span_id": span_id,
            "name": name,
            "kind": kind,
            "start_time_unix_nano": st,
            "end_time_unix_nano": et,
            "attributes": attrs,
        }
        if parent_id:
            span_obj["parent_span_id"] = parent_id
        if events:
            span_obj["events"] = events
        if links:
            span_obj["links"] = links

        spans_out.append(span_obj)
        span_count += 1

    spans_out.sort(key=lambda x: (trace_id, x["span_id"]))
    traces_out.append({"trace_id": trace_id, "spans": spans_out})

traces_out.sort(key=lambda x: x["trace_id"])

report = {
    "schema_version": "otel_bridge_report_v1",
    "report_version": "1",
    "assay_version": assay_version,
    "source": {"kind": "otel"},
    "summary": {"trace_count": len(traces_out), "span_count": span_count},
    "traces": traces_out,
    "extensions": {
        "non_attribute_metadata": {
            "resource": data.get("resource", None)
        }
    },
}

with open(out_json, "w", encoding="utf-8") as f:
    json.dump(report, f, indent=2, sort_keys=True)

md = []
md.append("# ADR-025 OTel Bridge Report (v1)")
md.append("")
md.append(f"- trace_count: **{report['summary']['trace_count']}**")
md.append(f"- span_count: **{report['summary']['span_count']}**")
md.append("")
md.append("## Notes")
md.append("- Unknown OTel attributes are preserved in `attributes[]` entries.")
md.append("- `extensions` is reserved for non-attribute metadata (e.g., resource).")
with open(out_md, "w", encoding="utf-8") as f:
    f.write("\n".join(md) + "\n")

raise SystemExit(0)
PY
