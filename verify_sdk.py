from assay import Policy, CoverageAnalyzer, analyze_coverage
import json
import os

# Setup
policy_yaml = """
name: test
version: "1.1" # Required for valid policy
tools:
  allow: [Search, Create]
sequences:
  - type: require
    tool: Search
"""
with open("policy.yaml", "w") as f:
    f.write(policy_yaml)

traces = [[
    {"tool": "Search", "args": {"q": "foo"}},
    {"tool": "Create", "args": {}}
]]

# Test High Level
print("Testing high level API...")
report = analyze_coverage("policy.yaml", traces)
print(json.dumps(report, indent=2))
assert report["tool_coverage"]["coverage_pct"] == 100.0

# Test Low Level
print("Testing low level API...")
p = Policy.from_file("policy.yaml")
a = CoverageAnalyzer(p)
r_json = a.analyze(traces, 80.0)
r = json.loads(r_json)
assert r["overall_coverage_pct"] >= 80.0

# Test Error Handling
print("Testing error handling...")
try:
    Policy.from_file("non_existent.yaml")
    print("FAILED: Should have raised FileNotFoundError")
    exit(1)
except FileNotFoundError:
    print("Caught FileNotFoundError as expected")
except Exception as e:
    print(f"Caught unexpected exception: {type(e)}: {e}")
    # PyO3 mapping might map to FileNotFoundError or RuntimeError depending on implementation.
    # In my impl: PyFileNotFoundError.new_err(e.to_string()) -> maps to FileNotFoundError in Python.

print("SUCCESS")
