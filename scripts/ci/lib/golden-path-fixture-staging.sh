# shellcheck shell=bash
# Single staging rule for the golden-path plugin fixture set shared by the
# hardening and optimization mutation suites (issue #2196 / CI-5C).

stage_golden_path_fixtures() {
  local case_root="$1"
  local repo_root="$2"

  mkdir -p \
    "$case_root/scripts/ci" \
    "$case_root/scripts/docs" \
    "$case_root/docs/generated" \
    "$case_root/examples/privileged-action-gate/policies" \
    "$case_root/.agents/skills/assay-golden-path" \
    "$case_root/.claude/skills/assay-golden-path" \
    "$case_root/.claude-plugin" \
    "$case_root/packaging/claude-plugin/.claude-plugin" \
    "$case_root/packaging/claude-plugin/skills/assay-golden-path/references" \
    "$case_root/packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/policies"

  cp "$repo_root/scripts/ci/test-agent-golden-path-skill.py" "$case_root/scripts/ci/"
  cp "$repo_root/scripts/docs/generate-agent-golden-path.py" "$case_root/scripts/docs/"
  cp "$repo_root/Cargo.toml" "$case_root/"
  cp "$repo_root/.gitignore" "$case_root/"
  cp "$repo_root/.gitattributes" "$case_root/"
  cp "$repo_root/.mcp.json" "$case_root/"
  cp "$repo_root/docs/generated/agent-golden-path.json" "$case_root/docs/generated/"
  cp "$repo_root/examples/privileged-action-gate/mock_github_mcp.py" \
    "$case_root/examples/privileged-action-gate/"
  cp "$repo_root/examples/privileged-action-gate/baseline-approved.json" \
    "$case_root/examples/privileged-action-gate/"
  cp "$repo_root/examples/privileged-action-gate/policies/no-allowance.yaml" \
    "$case_root/examples/privileged-action-gate/policies/"
  cp "$repo_root/.agents/skills/assay-golden-path/SKILL.md" \
    "$case_root/.agents/skills/assay-golden-path/"
  cp "$repo_root/.claude/skills/assay-golden-path/SKILL.md" \
    "$case_root/.claude/skills/assay-golden-path/"
  cp "$repo_root/.claude-plugin/marketplace.json" "$case_root/.claude-plugin/"
  cp "$repo_root/packaging/claude-plugin/.claude-plugin/plugin.json" \
    "$case_root/packaging/claude-plugin/.claude-plugin/"
  cp "$repo_root/packaging/claude-plugin/.mcp.json" "$case_root/packaging/claude-plugin/"
  cp "$repo_root/packaging/claude-plugin/skills/assay-golden-path/SKILL.md" \
    "$case_root/packaging/claude-plugin/skills/assay-golden-path/"
  cp "$repo_root/packaging/claude-plugin/skills/assay-golden-path/references/agent-golden-path.json" \
    "$case_root/packaging/claude-plugin/skills/assay-golden-path/references/"
  cp "$repo_root/packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/mock_github_mcp.py" \
    "$case_root/packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/"
  cp "$repo_root/packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/baseline-approved.json" \
    "$case_root/packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/"
  cp "$repo_root/packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/policies/no-allowance.yaml" \
    "$case_root/packaging/claude-plugin/skills/assay-golden-path/assets/privileged-action-gate/policies/"
}
