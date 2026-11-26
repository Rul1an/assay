#!/usr/bin/env bash
# Contract: perf_pr.yml keeps pull request code away from credentials.
#
# The measure job runs `cargo bench`, which builds and executes the pull request's code (build
# scripts, benches, the code under test). Before this contract that ran in the same step as
# `bencher run`, with BENCHER_API_TOKEN and a pull-requests: write GITHUB_TOKEN in its env, so the
# pull request's code could read both. A later step in the same job is not isolation (a background
# process, GITHUB_ENV or GITHUB_PATH carry over), so reporting runs in its own job: it holds the
# credentials, runs no code from the pull request, and reads the measure job's output as data.
#
#   (default)    validates the live workflow: static checks on the parsed YAML, then a behaviour
#                check. The behaviour check runs every measure-job step that calls cargo with a
#                stand-in cargo, with dummy credentials (never real ones) placed exactly where the
#                workflow declares them, and fails if the stand-in can read one. It then runs the
#                publish job's steps with a stand-in bencher on output files that carry shell
#                metacharacters, and checks they are passed as data with the measured commit.
#   --self-test  validates the live workflow, then proves each guard bites on a mutated copy; each
#                mutation must fail with its own message, not merely fail.
#
# Not claimed: an author who can edit this workflow can undo the split (that needs a separately
# trusted workflow, not another job in this one); the publish job trusts Bencher's adapter to
# parse the text it is given; the report is advisory and gates no merge.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/perf_pr.yml"
OTHER_WORKFLOWS=("${ROOT}/.github/workflows/perf_main.yml" "${ROOT}/.github/workflows/perf_nightly.yml")

validate() {
  ruby -ryaml -rjson -ropen3 -rtmpdir -rfileutils - "$1" "${OTHER_WORKFLOWS[@]}" <<'RUBY'
path, *others = ARGV

BENCHER = "bencherdev/bencher@b2484bc6734c8e6da4fad141bd13527a11da3988"
UPLOAD = "actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9"
DOWNLOAD = "actions/download-artifact@9000827ccba6bdab643e8b6fd33ac0654aef8333"
ARTIFACT = "perf-pr-bench-output-${{ github.run_id }}-${{ github.run_attempt }}"
DOWNLOAD_PATH = "${{ runner.temp }}/bench-output"
MEASURE = "benches"
PUBLISH = "bencher_publish"
ARCHIVE = "archive_pr_branch"
SAME_REPO = "github.event.pull_request.head.repo.full_name == github.repository"
NOT_CLOSED = "github.event.action != 'closed'"
HEAD_ASSERT = 'test "$(git rev-parse HEAD)" = "$GITHUB_SHA"'
BENCHES = {
  "store_write_heavy" => {
    "if" => "needs.detect-benchmark-relevance.outputs.store == 'true'",
    "cargo" => %q{cargo bench -p assay-core --bench store_write_heavy -- '^swc/' 2>&1},
    "testbed" => "ubuntu-latest-store-code",
  },
  "suite_run_worstcase" => {
    "if" => "needs.detect-benchmark-relevance.outputs.suite == 'true'",
    "cargo" => %q{cargo bench -p assay-cli --bench suite_run_worstcase 2>&1},
    "testbed" => "ubuntu-latest",
  },
}

def flags_for(id, testbed)
  [
    "--project", "$BENCHER_PROJECT", "--token", "$BENCHER_API_TOKEN", "--branch", "$GITHUB_HEAD_REF",
    "--hash", "$GITHUB_SHA", "--start-point", "main", "--start-point-clone-thresholds",
    "--start-point-reset", "--testbed", testbed, "--adapter", "rust_criterion", "--ci-id", id,
    "--github-actions", "$GITHUB_TOKEN", "--file", "$BENCH_OUTPUT",
  ]
end

problems = []
doc = YAML.safe_load_file(path, aliases: false)
jobs = doc.fetch("jobs")
refs_secret = ->(obj) { obj.to_s.match?(/secrets\.|github\.token/) }

# --- static: workflow level ----------------------------------------------------------------
problems << "workflow permissions must be exactly contents: read" unless doc["permissions"] == { "contents" => "read" }
problems << "workflow env must not reference secrets" if refs_secret.(doc["env"])
jobs.each do |name, job|
  next if [PUBLISH, ARCHIVE].include?(name)
  problems << "job #{name} must not reference secrets or github.token" if refs_secret.(job)
end
([path] + others).each do |wf|
  text = File.read(wf, encoding: "UTF-8")
  uses = text.scan(%r{bencherdev/bencher@[0-9a-f]+}).uniq
  problems << "#{File.basename(wf)}: bencherdev/bencher must be pinned to #{BENCHER.split("@").last}" unless uses == [BENCHER]
end

# --- static: measure job -------------------------------------------------------------------
measure = jobs[MEASURE]
if measure.nil?
  problems << "missing measure job #{MEASURE}"
else
  problems << "measure job permissions must be exactly contents: read" unless measure["permissions"] == { "contents" => "read" }
  problems << "measure job must not reference secrets or github.token" if refs_secret.(measure)
  steps = measure.fetch("steps")
  problems << "measure job must not run bencher" if steps.any? { |s| s["uses"].to_s.start_with?("bencherdev/") || s["run"].to_s.match?(/(^|[\s|;&(])bencher\s/) }
  checkout = steps.find { |s| s["uses"].to_s.start_with?("actions/checkout@") }
  problems << "measure job checkout must not persist credentials" unless checkout && checkout.dig("with", "persist-credentials") == false
  assert_step = steps.find { |s| s["run"].to_s.include?(HEAD_ASSERT) }
  problems << "measure job must assert the checked-out HEAD is GITHUB_SHA" unless assert_step && !assert_step.key?("if") && !assert_step.key?("continue-on-error") && steps.index(assert_step) > steps.index(checkout).to_i
  BENCHES.each do |id, spec|
    step = steps.find { |s| s["run"].to_s.include?(spec["cargo"]) }
    if step.nil?
      problems << "measure job is missing the #{id} benchmark step"
      next
    end
    problems << "benchmark steps must not set continue-on-error" if step.key?("continue-on-error")
    problems << "benchmark step #{id} must keep its relevance condition" unless step["if"] == spec["if"]
    problems << "benchmark step #{id} must write bench-output/#{id}.txt" unless step["run"].include?("> bench-output/#{id}.txt")
  end
  upload = steps.find { |s| s["uses"].to_s.start_with?("actions/upload-artifact@") }
  if upload.nil?
    problems << "measure job must upload its benchmark output"
  else
    problems << "upload must use the pinned upload-artifact" unless upload["uses"] == UPLOAD
    problems << "upload step must run only after every selected benchmark succeeded (no if, no continue-on-error)" if upload.key?("if") || upload.key?("continue-on-error")
    problems << "upload must be the measure job's last step" unless steps.last.equal?(upload)
    w = upload["with"] || {}
    problems << "upload must name the fixed per-attempt artifact" unless w["name"] == ARTIFACT
    problems << "upload must carry only bench-output/, fail when empty, and expire in a day" unless w["path"] == "bench-output/" && w["if-no-files-found"] == "error" && w["retention-days"].to_s == "1"
  end
end

# --- static: publish job -------------------------------------------------------------------
publish = jobs[PUBLISH]
if publish.nil?
  problems << "missing publish job #{PUBLISH}"
else
  needs = Array(publish["needs"])
  cond = publish["if"].to_s
  problems << "publish job must need benches and require its success" unless needs.include?(MEASURE) && needs.include?("detect-benchmark-relevance") && cond.include?("needs.benches.result == 'success'") && cond.include?(SAME_REPO) && cond.include?(NOT_CLOSED)
  problems << "publish job permissions must be exactly pull-requests: write" unless publish["permissions"] == { "pull-requests" => "write" }
  problems << "publish job env must not exist" if publish.key?("env")
  psteps = publish.fetch("steps")
  uses = psteps.map { |s| s["uses"] }.compact
  problems << "publish job may use only the pinned download-artifact and bencher actions" unless uses.sort == [BENCHER, DOWNLOAD].sort
  psteps.each do |s|
    run = s["run"].to_s
    if run.match?(/\bcargo\b|\bsource\b|\beval\b|(^|[\s;&|])\.\s+["$\/]|bash\s+["$]|sh\s+["$]|\bxargs\b/)
      problems << "publish job must not run cargo or execute artifact content"
    end
  end
  download = psteps.find { |s| s["uses"] == DOWNLOAD }
  if download
    problems << "download must name exactly the artifact the measure job uploads" unless download.dig("with", "name") == ARTIFACT
    problems << "download path must be the fixed runner temp directory" unless download.dig("with", "path") == DOWNLOAD_PATH
    problems << "download must not take a token, run id or repository" if (download["with"] || {}).keys.any? { |k| %w[github-token run-id repository pattern merge-multiple].include?(k) }
  end
  problems << "publish job must log the Bencher CLI version" unless psteps.any? { |s| s["run"].to_s.strip == "bencher --version" }
  BENCHES.each do |id, spec|
    step = psteps.find { |s| s["run"].to_s.include?("--ci-id #{id}") }
    if step.nil?
      problems << "publish job is missing the #{id} report step"
      next
    end
    problems << "publish step #{id} must not set continue-on-error" if step.key?("continue-on-error")
    problems << "publish step #{id} must run exactly when its benchmark was selected" unless step["if"] == spec["if"]
    problems << "publish step #{id} must read the fixed output file" unless step.dig("env", "BENCH_OUTPUT") == "#{DOWNLOAD_PATH}/#{id}.txt"
    run = step["run"].to_s
    test_at = run.index('test -s "$BENCH_OUTPUT"')
    run_at = run.index("bencher run")
    problems << "publish step for #{id} must refuse a missing or empty output file" unless test_at && run_at && test_at < run_at
    if run_at
      tokens = run[run_at..].gsub("\\\n", " ").split(/\s+/).map { |t| t.delete('"') }
      tokens = tokens[2..] || []
      problems << "publish flags for #{id} drifted" unless tokens == flags_for(id, spec["testbed"]).map { |t| t.delete('"') }
    end
  end
end

# --- behaviour -----------------------------------------------------------------------------
def expand(value, runner_temp)
  value.to_s.gsub(/\$\{\{\s*secrets\.([A-Za-z_]+)\s*\}\}/) { "dummy-secret-#{$1}" }
       .gsub(/\$\{\{\s*github\.token\s*\}\}/, "dummy-github-token")
       .gsub(/\$\{\{\s*runner\.temp\s*\}\}/, runner_temp)
       .gsub(/\$\{\{[^}]*\}\}/, "x")
end

def step_env(doc, job, step, runner_temp)
  [doc["env"], job["env"], step["env"]].compact.reduce({}) { |acc, e| acc.merge(e) }
    .to_h { |k, v| [k.to_s, expand(v, runner_temp)] }
end

STUB_CARGO = <<~'SH'
  #!/bin/bash
  env > "$STUB_LOG/cargo-env-$$-$RANDOM.txt"
  mkdir -p target/criterion/x/new && echo '{}' > target/criterion/x/new/estimates.json
  echo "swc/500x400b            time:   [1.0000 ms 1.1000 ms 1.2000 ms]"
SH
STUB_BENCHER = <<~'SH'
  #!/bin/bash
  if [ "${1:-}" = "--version" ]; then echo "bencher 0.6.17"; exit 0; fi
  [ -t 0 ] || cat > /dev/null
  printf '%s\n' "$@" > "$STUB_LOG/bencher-argv-$$-$RANDOM.txt"
SH
SHA = "0123456789abcdef0123456789abcdef01234567"

Dir.mktmpdir do |tmp|
  stubs = File.join(tmp, "bin"); FileUtils.mkdir_p(stubs)
  File.write(File.join(stubs, "cargo"), STUB_CARGO); File.chmod(0o755, File.join(stubs, "cargo"))
  File.write(File.join(stubs, "bencher"), STUB_BENCHER); File.chmod(0o755, File.join(stubs, "bencher"))
  base_env = { "PATH" => "#{stubs}:/usr/bin:/bin", "HOME" => tmp, "GITHUB_SHA" => SHA, "GITHUB_HEAD_REF" => "feature/x" }

  if measure
    log = File.join(tmp, "measure-log"); FileUtils.mkdir_p(log)
    ran = 0
    measure.fetch("steps").each do |s|
      next unless s["run"].to_s.match?(/\bcargo\b/)
      work = Dir.mktmpdir("work", tmp)
      env = base_env.merge(step_env(doc, measure, s, File.join(tmp, "runner-temp"))).merge("STUB_LOG" => log)
      Open3.capture2e(env, "bash", "--noprofile", "--norc", "-eo", "pipefail", "-c", s["run"], chdir: work, unsetenv_others: true)
      ran += 1
    end
    problems << "behaviour: no measure step runs cargo" if ran.zero?
    seen = Dir[File.join(log, "cargo-env-*")].flat_map { |f| File.readlines(f).grep(/=dummy-/).map { |l| l.split("=").first } }.uniq.sort
    problems << "behaviour: pull request code (stand-in cargo) can read credentials: #{seen.join(", ")}" unless seen.empty?
  end

  if publish
    runner_temp = File.join(tmp, "runner-temp"); out_dir = File.join(runner_temp, "bench-output")
    FileUtils.mkdir_p(out_dir)
    hostile = "swc/500x400b time: [1 ms 1 ms 1 ms]\n$(touch PWNED)\n`touch PWNED`\n; touch PWNED\n"
    BENCHES.each_key { |id| File.write(File.join(out_dir, "#{id}.txt"), hostile) }
    log = File.join(tmp, "publish-log"); FileUtils.mkdir_p(log)
    work = Dir.mktmpdir("pwork", tmp)
    publish.fetch("steps").each do |s|
      next unless s["run"]
      env = base_env.merge(step_env(doc, publish, s, runner_temp)).merge("STUB_LOG" => log)
      Open3.capture2e(env, "bash", "--noprofile", "--norc", "-eo", "pipefail", "-c", s["run"], chdir: work, unsetenv_others: true)
    end
    pwned = Dir.glob(File.join(tmp, "**", "PWNED"))
    problems << "behaviour: publish job executed artifact content" unless pwned.empty?
    argvs = Dir[File.join(log, "bencher-argv-*")].map { |f| File.readlines(f, chomp: true) }
    BENCHES.each_key do |id|
      file = File.join(out_dir, "#{id}.txt")
      hit = argvs.find { |a| a.include?(id) }
      problems << "behaviour: publish for #{id} must pass the output file and the measured commit as arguments" unless hit && hit.each_cons(2).include?(["--file", file]) && hit.each_cons(2).include?(["--hash", SHA])
      step = publish.fetch("steps").find { |s| s["run"].to_s.include?("--ci-id #{id}") }
      next unless step
      File.delete(file)
      env = base_env.merge(step_env(doc, publish, step, runner_temp)).merge("STUB_LOG" => log)
      _, status = Open3.capture2e(env, "bash", "--noprofile", "--norc", "-eo", "pipefail", "-c", step["run"], chdir: work, unsetenv_others: true)
      problems << "behaviour: publish step for #{id} must fail when its output file is missing" if status.success?
      File.write(file, hostile)
    end
  end
end

if problems.empty?
  puts "perf_pr credential boundary contract=passed"
else
  problems.uniq.each { |p| warn p }
  exit 1
end
RUBY
}

self_test() {
  validate "$WORKFLOW" >/dev/null
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  local count=0

  run_mutation() {
    local name="$1" expected="$2" out
    local mutated="${tmp}/${name}.yml"
    ruby -ryaml - "$WORKFLOW" "$mutated" "$name" <<'RUBY'
src, dst, mutation = ARGV
doc = YAML.safe_load_file(src, aliases: false)
jobs = doc.fetch("jobs")
measure = jobs.fetch("benches")
publish = jobs.fetch("bencher_publish")
msteps = measure.fetch("steps")
psteps = publish.fetch("steps")
store = msteps.find { |s| s["run"].to_s.include?("--bench store_write_heavy") }
pstore = psteps.find { |s| s["run"].to_s.include?("--ci-id store_write_heavy") }
upload = msteps.find { |s| s["uses"].to_s.start_with?("actions/upload-artifact@") }
download = psteps.find { |s| s["uses"].to_s.start_with?("actions/download-artifact@") }
case mutation
when "secret-in-bench-step" then (store["env"] ||= {})["BENCHER_API_TOKEN"] = "${{ secrets.BENCHER_API_TOKEN }}"
when "secret-in-bench-job" then (measure["env"] ||= {})["BENCHER_API_TOKEN"] = "${{ secrets.BENCHER_API_TOKEN }}"
when "token-in-bench-step" then (store["env"] ||= {})["GH"] = "${{ github.token }}"
when "secret-in-workflow-env" then (doc["env"] ||= {})["BENCHER_API_TOKEN"] = "${{ secrets.BENCHER_API_TOKEN }}"
when "write-on-measure" then measure["permissions"]["pull-requests"] = "write"
when "write-on-workflow" then doc["permissions"]["pull-requests"] = "write"
when "bencher-in-measure" then msteps.insert(msteps.index(upload), { "uses" => "bencherdev/bencher@b2484bc6734c8e6da4fad141bd13527a11da3988" })
when "no-head-assert" then msteps.reject! { |s| s["run"].to_s.include?('test "$(git rev-parse HEAD)" = "$GITHUB_SHA"') }
when "soft-bench" then store["continue-on-error"] = true
when "upload-always" then upload["if"] = "always()"
when "upload-soft" then upload["continue-on-error"] = true
when "upload-empty-ok" then upload["with"]["if-no-files-found"] = "warn"
when "artifact-name-drift" then download["with"]["name"] = "perf-pr-bench-output"
when "download-path-drift" then download["with"]["path"] = "bench-output"
when "download-other-run" then download["with"]["run-id"] = "${{ github.event.workflow_run.id }}"
when "checkout-in-publish" then psteps.unshift({ "uses" => "actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09" })
when "setup-rust-in-publish" then psteps.unshift({ "uses" => "./.github/actions/setup-rust" })
when "cache-in-publish" then psteps.unshift({ "uses" => "Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0" })
when "cargo-in-publish" then psteps.push({ "name" => "x", "shell" => "bash", "run" => "cargo --version" })
when "source-artifact" then pstore["run"] = pstore["run"].sub("test -s", "source \"$BENCH_OUTPUT\"\ntest -s")
when "publish-write-more" then publish["permissions"]["actions"] = "write"
when "publish-no-needs" then publish["needs"] = ["detect-benchmark-relevance"]
when "publish-ignores-result" then publish["if"] = publish["if"].sub(" && needs.benches.result == 'success'", "")
when "present-only" then pstore["run"] = pstore["run"].sub("test -s \"$BENCH_OUTPUT\"\n", "")
when "missing-hash" then pstore["run"] = pstore["run"].sub(" --hash \"$GITHUB_SHA\" \\\n", "\n").sub("--hash \"$GITHUB_SHA\" \\\n", "")
when "testbed-drift" then pstore["run"] = pstore["run"].sub("ubuntu-latest-store-code", "ubuntu-latest")
when "publish-soft-step" then pstore["continue-on-error"] = true
when "pin-drift" then psteps.each { |s| s["uses"] = "bencherdev/bencher@0f8f620172ccd6225d40a7590598eb7b41718af8" if s["uses"].to_s.start_with?("bencherdev/bencher@") }
else abort "unknown mutation #{mutation}"
end
File.write(dst, YAML.dump(doc))
RUBY
    if out="$(validate "$mutated" 2>&1)"; then
      echo "mutation did not bite: $name" >&2
      exit 1
    fi
    if [[ "$out" != *"$expected"* ]]; then
      echo "mutation $name failed without the expected message ($expected): $out" >&2
      exit 1
    fi
    count=$((count + 1))
    echo "ok    $name"
  }

  run_mutation secret-in-bench-step "measure job must not reference secrets or github.token"
  run_mutation secret-in-bench-job "measure job must not reference secrets or github.token"
  run_mutation token-in-bench-step "measure job must not reference secrets or github.token"
  run_mutation secret-in-workflow-env "workflow env must not reference secrets"
  run_mutation write-on-measure "measure job permissions must be exactly contents: read"
  run_mutation write-on-workflow "workflow permissions must be exactly contents: read"
  run_mutation bencher-in-measure "measure job must not run bencher"
  run_mutation no-head-assert "measure job must assert the checked-out HEAD is GITHUB_SHA"
  run_mutation soft-bench "benchmark steps must not set continue-on-error"
  run_mutation upload-always "upload step must run only after every selected benchmark succeeded"
  run_mutation upload-soft "upload step must run only after every selected benchmark succeeded"
  run_mutation upload-empty-ok "upload must carry only bench-output/, fail when empty"
  run_mutation artifact-name-drift "download must name exactly the artifact the measure job uploads"
  run_mutation download-path-drift "download path must be the fixed runner temp directory"
  run_mutation download-other-run "download must not take a token, run id or repository"
  run_mutation checkout-in-publish "publish job may use only the pinned download-artifact and bencher actions"
  run_mutation setup-rust-in-publish "publish job may use only the pinned download-artifact and bencher actions"
  run_mutation cache-in-publish "publish job may use only the pinned download-artifact and bencher actions"
  run_mutation cargo-in-publish "publish job must not run cargo or execute artifact content"
  run_mutation source-artifact "publish job must not run cargo or execute artifact content"
  run_mutation publish-write-more "publish job permissions must be exactly pull-requests: write"
  run_mutation publish-no-needs "publish job must need benches and require its success"
  run_mutation publish-ignores-result "publish job must need benches and require its success"
  run_mutation present-only "publish step for store_write_heavy must refuse a missing or empty output file"
  run_mutation missing-hash "publish flags for store_write_heavy drifted"
  run_mutation testbed-drift "publish flags for store_write_heavy drifted"
  run_mutation publish-soft-step "publish step store_write_heavy must not set continue-on-error"
  run_mutation pin-drift "bencherdev/bencher must be pinned to"
  echo "perf_pr credential boundary contract self-test=passed (${count} mutations)"
}

case "${1:-}" in
  --self-test) self_test ;;
  "") validate "$WORKFLOW" ;;
  *) validate "$1" ;;
esac
