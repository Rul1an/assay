#!/usr/bin/env bash
# Pin editor-mcp-recipe-truth and release-surface through the pre-commit consumer.
#
# The 38/95 suites parse hook YAML themselves and miss last-key-wins duplicates
# (`files: ^$`, `entry: 'true'`) and miss a second hook whose `id` is not written
# as `      - id: <name>`. This contract loads `.pre-commit-config.yaml` with a
# duplicate-key-rejecting PyYAML SafeLoader and counts both protected IDs from
# repos[*].hooks[*] (flow mapping and reordered keys included). Required context
# `CI` (the ci.yml aggregator) invokes this script directly; Kernel Matrix is
# not a required context, so a lint-job invocation cannot close the merge gate.
# One local consumer hook remains for pre-push feedback (`stages: [pre-push]`)
# so default `pre-commit run --all-files` does not duplicate the heavy check.
set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONFIG="$ROOT/.pre-commit-config.yaml"
CI_WORKFLOW="$ROOT/.github/workflows/ci.yml"
RULESET="$ROOT/.github/rulesets/main-required-ci-contexts.json"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

EDITOR_ID="editor-mcp-recipe-truth"
RELEASE_ID="release-surface"
CONSUMER_ID="editor-release-hook-precommit-consumer"
ABSENT_PRODUCER_ID="editor-release-hook-precommit-producer"
CONTRACT_REL="scripts/ci/test-editor-release-hook-precommit-consumer.sh"

EDITOR_ENTRY="bash -c 'bash scripts/ci/test-editor-plugin-install-commands.sh && bash scripts/ci/test-check-editor-mcp-recipe-truth.sh && bash scripts/ci/check-editor-mcp-recipe-truth.sh'"
RELEASE_ENTRY="bash -c 'bash scripts/ci/test-check-release-surface.sh && bash scripts/ci/check-release-surface.sh'"
CONSUMER_ENTRY="bash $CONTRACT_REL"

EDITOR_DROP="bash scripts/ci/test-editor-plugin-install-commands.sh && "
RELEASE_DROP="bash scripts/ci/test-check-release-surface.sh && "

failures=0

fail() {
  failures=$((failures + 1))
  printf 'FAIL: %s\n' "$*"
}

ok() { printf 'ok   %s\n' "$*"; }

if ! command -v pre-commit >/dev/null 2>&1; then
  fail "pre-commit not on PATH (CI pin is pre-commit==4.4.0)"
  exit 1
fi
# Duplicate-key YAML load needs PyYAML. System python3 has it on Linux CI;
# Darwin review machines may only expose it via the pre-commit interpreter.
if python3 -c 'import yaml' >/dev/null 2>&1; then
  PYTHON_YAML=python3
else
  PYTHON_YAML="$(sed -n '1s/^#!//p' "$(command -v pre-commit)")"
  if ! "$PYTHON_YAML" -c 'import yaml' >/dev/null 2>&1; then
    fail "PyYAML not importable from python3 or the pre-commit interpreter"
    exit 1
  fi
fi
CFG_PY="$TMP/editor_release_hook_precommit_consumer.py"
cat > "$CFG_PY" << 'ENDPY'
# Helper for test-editor-release-hook-precommit-consumer.sh (invoked via python3).
from __future__ import annotations

import re
import sys
from pathlib import Path

import yaml


class UniqueKeyLoader(yaml.SafeLoader):
    """SafeLoader that rejects repeated keys in the same mapping."""


def _construct_unique_mapping(loader, node, deep=False):
    if not isinstance(node, yaml.MappingNode):
        raise yaml.constructor.ConstructorError(
            None,
            None,
            f"expected a mapping node, but found {node.id}",
            node.start_mark,
        )
    loader.flatten_mapping(node)
    mapping = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in mapping:
            raise yaml.constructor.ConstructorError(
                "while constructing a mapping",
                node.start_mark,
                f"found duplicate key {key!r}",
                key_node.start_mark,
            )
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


UniqueKeyLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG,
    _construct_unique_mapping,
)


def load_unique(src: str):
    return yaml.load(src, Loader=UniqueKeyLoader)


def hook_starts(src: str, hook_id: str) -> list[int]:
    # Canonical live blocks are `      - id: <id>`. Mutations of those blocks
    # still use this locator; counting protected IDs does not.
    marker = f"      - id: {hook_id}\n"
    starts: list[int] = []
    i = 0
    while True:
        j = src.find(marker, i)
        if j < 0:
            return starts
        starts.append(j)
        i = j + 1


def hook_span(src: str, hook_id: str) -> tuple[int, int]:
    starts = hook_starts(src, hook_id)
    if len(starts) != 1:
        raise SystemExit(
            f"expected exactly one hook block for {hook_id}, found {len(starts)}"
        )
    start = starts[0]
    rest = src[start + 1 :]
    cuts = []
    for token in ("\n      - id:", "\n  - repo:"):
        k = rest.find(token)
        if k >= 0:
            cuts.append(k)
    end = start + 1 + min(cuts) if cuts else len(src)
    return start, end


def hook_block(src: str, hook_id: str) -> str:
    start, end = hook_span(src, hook_id)
    return src[start:end]


def replace_block(src: str, hook_id: str, new_block: str) -> str:
    start, end = hook_span(src, hook_id)
    return src[:start] + new_block.rstrip("\n") + "\n" + src[end:]


def insert_after_span(src: str, hook_id: str, extra: str) -> str:
    _, end = hook_span(src, hook_id)
    if not extra.endswith("\n"):
        extra += "\n"
    return src[:end] + extra + src[end:]


def insert_after_key(block: str, key: str, new_line: str) -> str:
    lines = block.splitlines(keepends=True)
    out: list[str] = []
    inserted = False
    for line in lines:
        out.append(line)
        code = line.split("#", 1)[0]
        if not inserted and code.strip().startswith(f"{key}:"):
            indent = line[: len(line) - len(line.lstrip(" "))]
            out.append(f"{indent}{new_line}\n")
            inserted = True
    if not inserted:
        raise SystemExit(f"no uncommented {key}: in hook block")
    return "".join(out)


def iter_hooks(data: object):
    if not isinstance(data, dict):
        return
    repos = data.get("repos")
    if not isinstance(repos, list):
        return
    for repo in repos:
        if not isinstance(repo, dict):
            continue
        hooks = repo.get("hooks")
        if not isinstance(hooks, list):
            continue
        for hook in hooks:
            if isinstance(hook, dict):
                yield hook


def hook_id_counts(data: object) -> dict[object, int]:
    counts: dict[object, int] = {}
    for hook in iter_hooks(data):
        hid = hook.get("id")
        if hid is None:
            continue
        counts[hid] = counts.get(hid, 0) + 1
    return counts


def hook_by_id(data: object, hook_id: str):
    found = [hook for hook in iter_hooks(data) if hook.get("id") == hook_id]
    if len(found) != 1:
        return None
    return found[0]


def problems_for(
    src: str,
    *,
    editor_id: str,
    release_id: str,
    consumer_id: str,
    absent_producer_id: str,
    editor_entry: str,
    release_entry: str,
    consumer_entry: str,
    contract_rel: str,
) -> list[str]:
    problems: list[str] = []
    try:
        data = load_unique(src)
    except yaml.YAMLError as exc:
        problems.append(f"pre-commit config YAML rejected: {exc}")
        return problems

    counts = hook_id_counts(data)
    for hook_id in (editor_id, release_id, consumer_id):
        n = counts.get(hook_id, 0)
        if n != 1:
            problems.append(f"{hook_id}: expected exactly one hook, found {n}")
    n_prod = counts.get(absent_producer_id, 0)
    if n_prod != 0:
        problems.append(
            f"{absent_producer_id}: expected no in-config producer, found {n_prod}"
        )

    required = {
        editor_id: {
            "entry": editor_entry,
            "paths": (
                "scripts/ci/test-editor-plugin-install-commands.sh",
                "scripts/ci/test-check-editor-mcp-recipe-truth.sh",
                "scripts/ci/check-editor-mcp-recipe-truth.sh",
                "scripts/ci/lib/editor-plugin-install-commands.sh",
                "docs/guides/editor-mcp-recipe.md",
                contract_rel,
            ),
        },
        release_id: {
            "entry": release_entry,
            "paths": (
                ".github/assay-release-tag",
                "scripts/ci/check-release-surface.sh",
                "scripts/ci/test-check-release-surface.sh",
                "docs/guides/editor-mcp-recipe.md",
                contract_rel,
            ),
        },
    }
    for hook_id, spec in required.items():
        if counts.get(hook_id, 0) != 1:
            continue
        hook = hook_by_id(data, hook_id)
        if hook is None:
            continue
        if "files" not in hook:
            problems.append(
                f"{hook_id}: expected exactly one uncommented files: selector, found 0"
            )
        else:
            files_val = hook.get("files")
            if files_val == "^$":
                problems.append(f"{hook_id}: files: selector is ^$")
            elif not isinstance(files_val, str):
                problems.append(f"{hook_id}: files: selector is not a string")
            else:
                pattern = re.compile(files_val)
                for path in spec["paths"]:
                    if not pattern.search(path):
                        problems.append(
                            f"{hook_id}: files: selector does not match {path}"
                        )
        if "entry" not in hook:
            problems.append(
                f"{hook_id}: expected exactly one uncommented entry:, found 0"
            )
        elif hook.get("entry") != spec["entry"]:
            problems.append(f"{hook_id}: entry is not the exact command chain")

    if counts.get(consumer_id, 0) == 1:
        hook = hook_by_id(data, consumer_id)
        if hook is not None:
            if hook.get("always_run") is not True:
                problems.append(f"{consumer_id}: must set always_run: true")
            if hook.get("pass_filenames") is not False:
                problems.append(f"{consumer_id}: must set pass_filenames: false")
            if hook.get("language") != "system":
                problems.append(f"{consumer_id}: must set language: system")
            if "files" in hook:
                problems.append(
                    f"{consumer_id}: must not have a files: start-condition regex"
                )
            stages = hook.get("stages")
            if stages != ["pre-push"]:
                problems.append(
                    f"{consumer_id}: stages must be exactly [pre-push], found {stages!r}"
                )
            if hook.get("entry") != consumer_entry:
                problems.append(f"{consumer_id}: entry must invoke only {contract_rel}")
            effective = hook.get("entry")
            if effective in ("true", True) or effective == "/usr/bin/true":
                problems.append(
                    f"{consumer_id}: effective entry is a no-op ({effective!r})"
                )
    return problems


CI_CONSUMER_STEP = (
    "      - name: Editor-release hook pre-commit consumer\n"
    "        shell: bash\n"
    "        run: |\n"
    "          set -euo pipefail\n"
    "          python -m pip install --user pre-commit==4.4.0\n"
    "          export PATH=\"$HOME/.local/bin:$PATH\"\n"
    "          bash scripts/ci/test-editor-release-hook-precommit-consumer.sh\n"
)
CI_CONSUMER_STEP_HEAD = (
    "      - name: Editor-release hook pre-commit consumer\n"
    "        shell: bash\n"
)
CI_CONSUMER_RUN_LINE = (
    "          bash scripts/ci/test-editor-release-hook-precommit-consumer.sh\n"
)

ALLOWED_PRELUDE = (
    "python -m pip install --user pre-commit==4.4.0",
    'export PATH="$HOME/.local/bin:$PATH"',
)


def ruleset_contexts(ruleset_src: str) -> set[str]:
    doc = yaml.safe_load(ruleset_src)
    found: set[str] = set()
    if not isinstance(doc, dict):
        return found
    for rule in doc.get("rules") or []:
        if not isinstance(rule, dict) or rule.get("type") != "required_status_checks":
            continue
        params = rule.get("parameters")
        if not isinstance(params, dict):
            continue
        for check in params.get("required_status_checks") or []:
            if isinstance(check, dict) and isinstance(check.get("context"), str):
                found.add(check["context"])
    return found


def step_uncommented_commands(run: str) -> tuple[str, ...]:
    return tuple(
        line.strip()
        for line in run.splitlines()
        if line.strip()
        and not line.strip().startswith("#")
        and line.strip() != "set -euo pipefail"
    )


def problems_for_ci(src: str, *, contract_rel: str, ruleset_src: str) -> list[str]:
    problems: list[str] = []
    try:
        data = load_unique(src)
    except yaml.YAMLError as exc:
        problems.append(f"ci.yml YAML rejected: {exc}")
        return problems
    if not isinstance(data, dict):
        problems.append("ci.yml: root is not a mapping")
        return problems
    jobs = data.get("jobs")
    if not isinstance(jobs, dict):
        problems.append("ci.yml: jobs missing")
        return problems
    contexts = ruleset_contexts(ruleset_src)
    if "CI" not in contexts:
        problems.append("ruleset missing required context CI")
        return problems

    wanted = f"bash {contract_rel}"
    active: list[tuple[str, str, bool]] = []
    for job_id, job in jobs.items():
        if not isinstance(job, dict):
            continue
        job_name = job.get("name") if isinstance(job.get("name"), str) else job_id
        job_coe = job.get("continue-on-error") is True
        steps = job.get("steps")
        if not isinstance(steps, list):
            continue
        for step in steps:
            if not isinstance(step, dict):
                continue
            run = step.get("run")
            if not isinstance(run, str):
                continue
            body = step_uncommented_commands(run)
            if wanted not in body:
                continue
            extra = tuple(cmd for cmd in body if cmd != wanted)
            if any(cmd not in ALLOWED_PRELUDE for cmd in extra):
                continue
            if step.get("continue-on-error") is True:
                continue
            if "if" in step:
                continue
            active.append((job_id, job_name, job_coe))

    if len(active) != 1:
        problems.append(
            f"ci.yml: expected exactly one active direct invocation of "
            f"{contract_rel} in a required-context job, found {len(active)}"
        )
        return problems
    job_id, job_name, job_coe = active[0]
    if job_name not in contexts:
        problems.append(
            f"ci.yml: invocation is in job {job_id!r} named {job_name!r}, "
            f"which is not a required context {sorted(contexts)}"
        )
    if job_coe:
        problems.append(
            f"ci.yml: job {job_id!r} sets continue-on-error, so a consumer "
            "failure would not redden the required context"
        )
    return problems


def main() -> None:
    (
        editor_id,
        release_id,
        consumer_id,
        absent_producer_id,
        editor_entry,
        release_entry,
        consumer_entry,
        contract_rel,
        action,
        config_path,
        dest,
        *rest
    ) = sys.argv[1:]
    src = Path(config_path).read_text(encoding="utf-8")

    if action == "check":
        problems = problems_for(
            src,
            editor_id=editor_id,
            release_id=release_id,
            consumer_id=consumer_id,
            absent_producer_id=absent_producer_id,
            editor_entry=editor_entry,
            release_entry=release_entry,
            consumer_entry=consumer_entry,
            contract_rel=contract_rel,
        )
        if problems:
            sys.stderr.write("\n".join(problems) + "\n")
            raise SystemExit(1)
        return

    if action == "check-ci":
        ruleset_path = rest[0] if rest else dest
        ruleset_src = Path(ruleset_path).read_text(encoding="utf-8")
        problems = problems_for_ci(
            src, contract_rel=contract_rel, ruleset_src=ruleset_src
        )
        if problems:
            sys.stderr.write("\n".join(problems) + "\n")
            raise SystemExit(1)
        return

    if action == "check-trusted-prefix":
        import importlib.util
        checker_path = rest[0]
        spec = importlib.util.spec_from_file_location(
            "conformance_inventory_callsite", checker_path
        )
        if spec is None or spec.loader is None:
            raise SystemExit(f"cannot load {checker_path}")
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        problems = mod.hardening_guard_callsite_problems(src)
        if problems:
            sys.stderr.write("\n".join(problems) + "\n")
            raise SystemExit(1)
        return

    if action == "check-review-record-root":
        import importlib.util
        checker_path, workflow_path, host_path = rest[0], rest[1], rest[2]
        spec = importlib.util.spec_from_file_location(
            "review_record_workflow", checker_path
        )
        if spec is None or spec.loader is None:
            raise SystemExit(f"cannot load {checker_path}")
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        problems = mod.contract_problems(
            Path(workflow_path).read_text(encoding="utf-8"),
            src,
            Path(host_path).read_text(encoding="utf-8"),
        )
        if problems:
            sys.stderr.write("\n".join(problems) + "\n")
            raise SystemExit(1)
        return

    if action == "mutate-ci":
        kind = rest[0]
        if src.count(CI_CONSUMER_STEP) != 1:
            raise SystemExit("canonical CI consumer step is not unique")
        if kind == "delete-step":
            Path(dest).write_text(src.replace(CI_CONSUMER_STEP, "", 1), encoding="utf-8")
            return
        if kind == "comment-step":
            if src.count(CI_CONSUMER_RUN_LINE) != 1:
                raise SystemExit("canonical CI consumer run line is not unique")
            Path(dest).write_text(
                src.replace(
                    CI_CONSUMER_RUN_LINE,
                    "          # bash scripts/ci/test-editor-release-hook-precommit-consumer.sh\n",
                    1,
                ),
                encoding="utf-8",
            )
            return
        if kind == "if-false":
            if src.count(CI_CONSUMER_STEP_HEAD) != 1:
                raise SystemExit("canonical CI consumer step head is not unique")
            Path(dest).write_text(
                src.replace(
                    CI_CONSUMER_STEP_HEAD,
                    CI_CONSUMER_STEP_HEAD + "        if: false\n",
                    1,
                ),
                encoding="utf-8",
            )
            return
        if kind == "continue-on-error":
            if src.count(CI_CONSUMER_STEP_HEAD) != 1:
                raise SystemExit("canonical CI consumer step head is not unique")
            Path(dest).write_text(
                src.replace(
                    CI_CONSUMER_STEP_HEAD,
                    CI_CONSUMER_STEP_HEAD + "        continue-on-error: true\n",
                    1,
                ),
                encoding="utf-8",
            )
            return
        if kind == "before-hardening":
            heading = "      - name: Verify CI hardening contracts\n"
            if src.count(heading) != 1:
                raise SystemExit("hardening step heading is not unique")
            without = src.replace(CI_CONSUMER_STEP, "", 1)
            if without.count(heading) != 1:
                raise SystemExit("hardening heading lost after consumer removal")
            Path(dest).write_text(
                without.replace(heading, CI_CONSUMER_STEP + "\n" + heading, 1),
                encoding="utf-8",
            )
            return
        if kind == "after-finale":
            heading = "      - name: Verify review-record workflow contract\n"
            if src.count(heading) != 1:
                raise SystemExit("review-record root heading is not unique")
            without = src.replace(CI_CONSUMER_STEP, "", 1)
            if without.count(heading) != 1:
                raise SystemExit("review-record heading lost after consumer removal")
            Path(dest).write_text(
                without.replace(heading, CI_CONSUMER_STEP + "\n" + heading, 1),
                encoding="utf-8",
            )
            return
        raise SystemExit(f"unknown ci mutation {kind}")

    if action == "extract-minimal":
        hook_id = rest[0]
        block = hook_block(src, hook_id)
        Path(dest).write_text(
            'minimum_pre_commit_version: "4.4.0"\n'
            "repos:\n"
            "  - repo: local\n"
            "    hooks:\n" + block.rstrip("\n") + "\n",
            encoding="utf-8",
        )
        return

    if action == "mutate":
        kind, hook_id = rest[0], rest[1]
        if kind == "second-files":
            block = insert_after_key(hook_block(src, hook_id), "files", "files: ^$")
            Path(dest).write_text(replace_block(src, hook_id, block), encoding="utf-8")
            return
        if kind == "entry-true":
            block = insert_after_key(hook_block(src, hook_id), "entry", "entry: 'true'")
            Path(dest).write_text(replace_block(src, hook_id, block), encoding="utf-8")
            return
        if kind == "drop-command":
            token = rest[2]
            block = hook_block(src, hook_id)
            if token not in block:
                raise SystemExit(f"drop token not found in {hook_id}: {token!r}")
            block = block.replace(token, "", 1)
            Path(dest).write_text(replace_block(src, hook_id, block), encoding="utf-8")
            return
        if kind == "flow-duplicate":
            extra = (
                "      - {id: %s, entry: 'true', language: system}\n" % hook_id
            )
            Path(dest).write_text(insert_after_span(src, hook_id, extra), encoding="utf-8")
            return
        if kind == "reordered-duplicate":
            extra = (
                f"      - name: decoy {hook_id}\n"
                f"        entry: 'true'\n"
                f"        language: system\n"
                f"        id: {hook_id}\n"
            )
            Path(dest).write_text(insert_after_span(src, hook_id, extra), encoding="utf-8")
            return
        if kind == "drop-stages":
            block = hook_block(src, hook_id)
            lines = [
                line
                for line in block.splitlines(keepends=True)
                if not line.split("#", 1)[0].strip().startswith("stages:")
            ]
            Path(dest).write_text(
                replace_block(src, hook_id, "".join(lines)), encoding="utf-8"
            )
            return
        if kind == "change-stages":
            block = hook_block(src, hook_id)
            replaced = 0
            out = []
            for line in block.splitlines(keepends=True):
                code = line.split("#", 1)[0]
                if code.strip().startswith("stages:"):
                    indent = line[: len(line) - len(line.lstrip(" "))]
                    out.append(f"{indent}stages: [commit]\n")
                    replaced += 1
                else:
                    out.append(line)
            if replaced != 1:
                raise SystemExit(f"expected one stages: line in {hook_id}, found {replaced}")
            Path(dest).write_text(
                replace_block(src, hook_id, "".join(out)), encoding="utf-8"
            )
            return
        raise SystemExit(f"unknown mutation {kind}")

    raise SystemExit(f"unknown action {action}")


if __name__ == "__main__":
    main()
ENDPY

cfg() {
  "$PYTHON_YAML" "$CFG_PY" \
    "$EDITOR_ID" "$RELEASE_ID" "$CONSUMER_ID" "$ABSENT_PRODUCER_ID" \
    "$EDITOR_ENTRY" "$RELEASE_ENTRY" "$CONSUMER_ENTRY" \
    "$CONTRACT_REL" "$@"
}

check_wiring() {
  local path="$1"
  cfg check "$path" "$path"
}

unclose_plugin_fence() {
  python3 - "$1" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
needle = "# assay-editor-plugin-install-commands:end\n```\n"
if needle not in text:
    raise SystemExit("plugin bash-fence close not found")
path.write_text(
    text.replace(needle, "# assay-editor-plugin-install-commands:end\n", 1),
    encoding="utf-8",
)
PY
}

git_seed() {
  local dest="$1"
  git -C "$dest" init -q
  git -C "$dest" config user.email "ci@example.com"
  git -C "$dest" config user.name "CI"
  git -C "$dest" config commit.gpgsign false
  git -C "$dest" add -A
  git -C "$dest" commit -q -m seed
}

archive_into() {
  local dest="$1"
  shift
  mkdir -p "$dest"
  git -C "$ROOT" archive HEAD -- "$@" | tar -x -C "$dest"
}

prepare_editor_tree() {
  local dest="$1"
  archive_into "$dest" \
    scripts/ci/test-editor-plugin-install-commands.sh \
    scripts/ci/test-check-editor-mcp-recipe-truth.sh \
    scripts/ci/check-editor-mcp-recipe-truth.sh \
    scripts/ci/lib/editor-plugin-install-commands.sh \
    scripts/ci/lib/clear-git-repository-env.sh \
    docs/guides/editor-mcp-recipe.md \
    .github/workflows/ci.yml
}

prepare_release_tree() {
  local dest="$1"
  local paths=""
  paths="$(
    git -C "$ROOT" ls-files \
      Cargo.toml Cargo.lock \
      'crates/*/Cargo.toml' assay-python-sdk/Cargo.toml \
      README.md SECURITY.md llms.txt mkdocs.yml \
      docs/getting-started/installation.md \
      docs/getting-started/index.md \
      docs/getting-started/quickstart.md \
      docs/getting-started/ci-integration.md \
      docs/reference/cli/index.md \
      docs/AIcontext/user-flows.md \
      docs/use-cases/ci-gate.md \
      docs/use-cases/air-gapped.md \
      docs/index.md docs/COMMUNITY.md \
      docs/python-sdk/index.md docs/migration-v1.2.md \
      docs/guides/editor-mcp-recipe.md \
      examples/mcp-quickstart/README.md \
      .devcontainer/welcome.sh demo/CODESPACES-PLAYBOOK.md \
      .github/assay-release-tag \
      scripts/ci/check-release-surface.sh \
      scripts/ci/test-check-release-surface.sh \
      scripts/ci/read-assay-release-tag.sh \
      scripts/ci/release_readme.py \
      scripts/ci/lib/editor-plugin-install-commands.sh \
      scripts/ci/lib/clear-git-repository-env.sh
  )"
  # Paths are repo-relative and space-free; keep this split for bash 3.2.
  # shellcheck disable=SC2086
  archive_into "$dest" $paths
}

install_hook_config() {
  local src_config="$1" hook_id="$2" dest="$3"
  cfg extract-minimal "$src_config" "$dest/.pre-commit-config.yaml" "$hook_id"
}

consumer_status() {
  local dest="$1"
  local hook_id="$2"
  local out="$dest/precommit.out"
  set +e
  (
    cd "$dest"
    export PRE_COMMIT_HOME="$dest/pc-home"
    if [ "$hook_id" = "$CONSUMER_ID" ]; then
      pre-commit run "$hook_id" --hook-stage pre-push --color never --all-files
    else
      pre-commit run "$hook_id" --color never --all-files
    fi
  ) >"$out" 2>&1
  local rc=$?
  set -e
  if grep -Fq 'Skipped' "$out"; then
    printf 'skipped'
    return 0
  fi
  if grep -Fq 'Passed' "$out" && [ "$rc" -eq 0 ]; then
    printf 'passed'
    return 0
  fi
  if [ "$rc" -ne 0 ]; then
    printf 'failed'
    return 0
  fi
  printf 'unknown'
}

expect_wiring_red() {
  local label="$1" config="$2"
  if check_wiring "$config" >"$TMP/$label.wiring" 2>&1; then
    fail "$label — wiring stayed green"
  else
    ok "RED $label"
  fi
}

check_ci_producer() {
  local path="$1"
  cfg check-ci "$path" "$path" "$RULESET"
}

CALLSITE="$ROOT/scripts/ci/check-conformance-inventory-callsite.py"
REVIEW_RECORD_CHECKER="$ROOT/scripts/ci/check-review-record-workflow.py"
REVIEW_RECORD_WORKFLOW="$ROOT/.github/workflows/review-record-check.yml"
HOST_WORKFLOW="$ROOT/.github/workflows/host-capability-check.yml"

check_trusted_prefix() {
  local path="$1"
  cfg check-trusted-prefix "$path" "$path" "$CALLSITE"
}

expect_prefix_red() {
  local label="$1" config="$2"
  if check_trusted_prefix "$config" >"$TMP/$label.prefix" 2>&1; then
    fail "$label — trusted-prefix checker stayed green"
  else
    ok "RED $label"
  fi
}

check_review_record_root() {
  local path="$1"
  cfg check-review-record-root "$path" "$path" \
    "$REVIEW_RECORD_CHECKER" "$REVIEW_RECORD_WORKFLOW" "$HOST_WORKFLOW"
}

expect_review_record_red() {
  local label="$1" config="$2"
  if check_review_record_root "$config" >"$TMP/$label.review-record" 2>&1; then
    fail "$label — review-record checker stayed green"
  else
    ok "RED $label"
  fi
}

expect_ci_red() {
  local label="$1" config="$2"
  if check_ci_producer "$config" >"$TMP/$label.ci" 2>&1; then
    fail "$label — CI producer stayed green"
  else
    ok "RED $label"
  fi
}

expect_consumer() {
  local label="$1" dest="$2" hook_id="$3" want="$4"
  local got
  got="$(consumer_status "$dest" "$hook_id")"
  if [ "$got" = "$want" ]; then
    ok "$label (consumer $got)"
  else
    fail "$label — consumer $got, wanted $want"
    sed 's/^/      /' "$dest/precommit.out" || true
  fi
}

echo '== live wiring =='
if check_wiring "$CONFIG"; then
  ok 'live unmutated tree wiring'
else
  fail 'live unmutated tree wiring'
fi

echo '== live required-CI producer =='
if check_ci_producer "$CI_WORKFLOW"; then
  ok 'live CI job direct consumer invocation reaches required context'
else
  fail 'live CI job direct consumer invocation reaches required context'
fi

echo '== live trusted prefix =='
if check_trusted_prefix "$CI_WORKFLOW"; then
  ok 'live hardening step follows checkout + setup-python'
else
  fail 'live hardening step follows checkout + setup-python'
fi

echo '== live review-record root =='
if check_review_record_root "$CI_WORKFLOW"; then
  ok 'live review-record root immediately follows finale'
else
  fail 'live review-record root immediately follows finale'
fi

echo '== pin behavior: unmutated consumer must fail closed =='
teeth_editor="$TMP/teeth-editor"
prepare_editor_tree "$teeth_editor"
unclose_plugin_fence "$teeth_editor/docs/guides/editor-mcp-recipe.md"
install_hook_config "$CONFIG" "$EDITOR_ID" "$teeth_editor"
git_seed "$teeth_editor"
expect_consumer 'malformed recipe (missing plugin bash-fence close)' \
  "$teeth_editor" "$EDITOR_ID" failed

teeth_release="$TMP/teeth-release"
prepare_release_tree "$teeth_release"
printf '%s\n' '0.0.0' > "$teeth_release/.github/assay-release-tag"
install_hook_config "$CONFIG" "$RELEASE_ID" "$teeth_release"
git_seed "$teeth_release"
expect_consumer 'stale pin 0.0.0' "$teeth_release" "$RELEASE_ID" failed

echo '== named wiring mutations =='
cfg mutate "$CONFIG" "$TMP/second-files-editor.yaml" second-files "$EDITOR_ID"
expect_wiring_red 'second files: ^$ on editor hook' "$TMP/second-files-editor.yaml"

cfg mutate "$CONFIG" "$TMP/second-files-release.yaml" second-files "$RELEASE_ID"
expect_wiring_red 'second files: ^$ on release-surface hook' "$TMP/second-files-release.yaml"

cfg mutate "$CONFIG" "$TMP/entry-true-editor.yaml" entry-true "$EDITOR_ID"
expect_wiring_red 'entry: true on editor hook' "$TMP/entry-true-editor.yaml"

cfg mutate "$CONFIG" "$TMP/entry-true-release.yaml" entry-true "$RELEASE_ID"
expect_wiring_red 'entry: true on release-surface hook' "$TMP/entry-true-release.yaml"

cfg mutate "$CONFIG" "$TMP/drop-editor.yaml" drop-command "$EDITOR_ID" "$EDITOR_DROP"
expect_wiring_red 'remove test-editor-plugin-install-commands.sh from editor chain' \
  "$TMP/drop-editor.yaml"

cfg mutate "$CONFIG" "$TMP/drop-release.yaml" drop-command "$RELEASE_ID" "$RELEASE_DROP"
expect_wiring_red 'remove test-check-release-surface.sh from release-surface chain' \
  "$TMP/drop-release.yaml"

cfg mutate "$CONFIG" "$TMP/entry-true-consumer.yaml" entry-true "$CONSUMER_ID"
expect_wiring_red "last-key-wins entry: 'true' on consumer hook" \
  "$TMP/entry-true-consumer.yaml"

cfg mutate "$TMP/entry-true-consumer.yaml" "$TMP/entry-true-editor-too.yaml" \
  entry-true "$EDITOR_ID"
cfg mutate "$TMP/entry-true-editor-too.yaml" "$TMP/entry-true-paired.yaml" \
  entry-true "$RELEASE_ID"
expect_wiring_red "paired last-key-wins entry: 'true' on consumer, editor, and release-surface" \
  "$TMP/entry-true-paired.yaml"

cfg mutate "$CONFIG" "$TMP/drop-stages.yaml" drop-stages "$CONSUMER_ID"
expect_wiring_red 'removing pre-push stage from consumer hook' "$TMP/drop-stages.yaml"

cfg mutate "$CONFIG" "$TMP/change-stages.yaml" change-stages "$CONSUMER_ID"
expect_wiring_red 'changing consumer hook stage away from pre-push' \
  "$TMP/change-stages.yaml"

cfg mutate-ci "$CI_WORKFLOW" "$TMP/ci-delete.yml" delete-step
expect_ci_red 'deleting required-CI consumer step' "$TMP/ci-delete.yml"

cfg mutate-ci "$CI_WORKFLOW" "$TMP/ci-comment.yml" comment-step
expect_ci_red 'commenting required-CI consumer step' "$TMP/ci-comment.yml"

cfg mutate-ci "$CI_WORKFLOW" "$TMP/ci-if-false.yml" if-false
expect_ci_red 'if: false on required-CI consumer step' "$TMP/ci-if-false.yml"

cfg mutate-ci "$CI_WORKFLOW" "$TMP/ci-continue.yml" continue-on-error
expect_ci_red 'continue-on-error on required-CI consumer step' \
  "$TMP/ci-continue.yml"

cfg mutate-ci "$CI_WORKFLOW" "$TMP/ci-before-hardening.yml" before-hardening
expect_prefix_red 'consumer step between setup-python and hardening' \
  "$TMP/ci-before-hardening.yml"

cfg mutate-ci "$CI_WORKFLOW" "$TMP/ci-after-finale.yml" after-finale
expect_review_record_red 'consumer step between finale and review-record root' \
  "$TMP/ci-after-finale.yml"

cfg mutate "$CONFIG" "$TMP/flow-dup-editor.yaml" flow-duplicate "$EDITOR_ID"
expect_wiring_red 'flow-mapping duplicate of editor-mcp-recipe-truth' \
  "$TMP/flow-dup-editor.yaml"

cfg mutate "$CONFIG" "$TMP/reordered-dup-release.yaml" reordered-duplicate "$RELEASE_ID"
expect_wiring_red 'reordered-key duplicate of release-surface' \
  "$TMP/reordered-dup-release.yaml"

echo '== malformed recipe paired with editor bypasses =='
pair="$TMP/pair-editor-second-files"
prepare_editor_tree "$pair"
unclose_plugin_fence "$pair/docs/guides/editor-mcp-recipe.md"
install_hook_config "$TMP/second-files-editor.yaml" "$EDITOR_ID" "$pair"
git_seed "$pair"
expect_consumer 'malformed recipe + second files: ^$ on editor hook' \
  "$pair" "$EDITOR_ID" skipped

pair="$TMP/pair-editor-entry-true"
prepare_editor_tree "$pair"
unclose_plugin_fence "$pair/docs/guides/editor-mcp-recipe.md"
install_hook_config "$TMP/entry-true-editor.yaml" "$EDITOR_ID" "$pair"
git_seed "$pair"
expect_consumer 'malformed recipe + entry: true on editor hook' \
  "$pair" "$EDITOR_ID" passed

echo '== stale pin paired with release-surface bypasses =='
pair="$TMP/pair-release-second-files"
prepare_release_tree "$pair"
printf '%s\n' '0.0.0' > "$pair/.github/assay-release-tag"
install_hook_config "$TMP/second-files-release.yaml" "$RELEASE_ID" "$pair"
git_seed "$pair"
expect_consumer 'stale pin 0.0.0 + second files: ^$ on release-surface hook' \
  "$pair" "$RELEASE_ID" skipped

pair="$TMP/pair-release-entry-true"
prepare_release_tree "$pair"
printf '%s\n' '0.0.0' > "$pair/.github/assay-release-tag"
install_hook_config "$TMP/entry-true-release.yaml" "$RELEASE_ID" "$pair"
git_seed "$pair"
expect_consumer 'stale pin 0.0.0 + entry: true on release-surface hook' \
  "$pair" "$RELEASE_ID" passed

echo '== default pre-commit run --all-files skips the pre-push consumer =='
skip="$TMP/all-files-skip"
mkdir -p "$skip"
install_hook_config "$CONFIG" "$CONSUMER_ID" "$skip"
git_seed "$skip"
set +e
(
  cd "$skip"
  export PRE_COMMIT_HOME="$skip/pc-home"
  pre-commit run --color never --all-files
) >"$skip/precommit.out" 2>&1
skip_rc=$?
set -e
if grep -Eq 'Passed|Failed' "$skip/precommit.out"; then
  fail 'default pre-commit run --all-files ran the pre-push consumer'
  sed 's/^/      /' "$skip/precommit.out" || true
elif [ "$skip_rc" -eq 0 ]; then
  ok 'default pre-commit run --all-files skips pre-push consumer'
else
  fail "default pre-commit run --all-files rc=$skip_rc"
  sed 's/^/      /' "$skip/precommit.out" || true
fi

echo '== consumer last-key-wins entry: true is a local no-op =='
pair="$TMP/pair-consumer-entry-true"
mkdir -p "$pair"
install_hook_config "$TMP/entry-true-consumer.yaml" "$CONSUMER_ID" "$pair"
git_seed "$pair"
expect_consumer "consumer hook last-key-wins entry: 'true' is a no-op" \
  "$pair" "$CONSUMER_ID" passed

printf '\n'
if [ "$failures" -gt 0 ]; then
  printf 'editor/release-surface pre-commit consumer: %s failure(s)\n' "$failures"
  exit 1
fi
printf 'editor/release-surface pre-commit consumer: PASS\n'
