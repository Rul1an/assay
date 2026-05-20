#!/usr/bin/env sh
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <work-dir>" >&2
  exit 64
fi

work_dir=$1
mkdir -p "$work_dir"

input_file="$work_dir/input.txt"
output_file="$work_dir/output.txt"

printf '%s\n' "assay runner kernel-only fixture input" > "$input_file"
cat "$input_file" >/dev/null
printf '%s\n' "assay runner kernel-only fixture output" > "$output_file"

/usr/bin/env ASSAY_RUNNER_KERNEL_ONLY_FIXTURE=1 >/dev/null
