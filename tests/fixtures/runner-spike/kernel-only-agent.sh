#!/usr/bin/env sh
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <work-dir> <loopback-port>" >&2
  exit 64
fi

work_dir=$1
loopback_port=$2
mkdir -p "$work_dir"

input_file="$work_dir/input.txt"
output_file="$work_dir/output.txt"
not_executable="$work_dir/not-an-executable"
report_file="$work_dir/fixture-report.json"

printf '%s\n' "assay runner kernel-only fixture input" > "$input_file"
cat "$input_file" >/dev/null
printf '%s\n' "assay runner kernel-only fixture output" > "$output_file"

# One loopback connect attempt and one exec syscall that the kernel rejects. Both are issued
# from Python so the syscalls happen for certain: a shell may reject a non-executable file
# before calling execve, or fall back to running it as a script.
python3 -I -S -B - "$loopback_port" "$not_executable" "$report_file" <<'PY'
import errno
import json
import os
import socket
import sys

port = int(sys.argv[1])
not_executable = sys.argv[2]
report_path = sys.argv[3]

with open(not_executable, "wb") as handle:
    handle.write(b"assay runner kernel-only fixture: not an executable image\n")
os.chmod(not_executable, 0o755)

report = {"schema": "assay.runner.kernel_only_fixture_report.v0", "loopback_port": port}

sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.settimeout(5)
try:
    sock.connect(("127.0.0.1", port))
finally:
    sock.close()
report["loopback_connect"] = "connected"

try:
    os.execv(not_executable, [not_executable])
except OSError as error:
    report["failed_exec_errno"] = errno.errorcode.get(error.errno, str(error.errno))

with open(report_path, "w", encoding="utf-8") as handle:
    json.dump(report, handle, sort_keys=True)
    handle.write("\n")
PY

/usr/bin/env ASSAY_RUNNER_KERNEL_ONLY_FIXTURE=1 >/dev/null
