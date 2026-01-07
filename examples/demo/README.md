# Assay Demo: Break & Fix

This directory demonstrates the **Autofix** workflow. You don't need to write YAML; let Assay do it.

## The Story
You have an MCP server. You started with a permissive config (`unsafe-policy.yaml`). Now you want to secure it.

## 1. The Problem
Run validation on the unsafe policy:

```bash
assay validate --config unsafe-policy.yaml
```

**Output:**
```text
✗ tool "exec" is allowed
  → potential RCE vulnerability
```

## 2. The Fix (Preview)
Ask Assay to suggest fixes without applying them:

```bash
assay fix --config unsafe-policy.yaml --dry-run
```

**Output:**
```diff
- deny: []
+ deny: ["exec", "shell", "spawn"]
```

## 3. Apply Fix
Apply the changes:

```bash
cp unsafe-policy.yaml assay.yaml
assay fix --yes
```

## 4. Verify
Run validation again:

```bash
assay validate
```

**Output:**
```text
✓ Policy is secure
```

> **Note**: `assay fix` modifies your policy configuration (`assay.yaml`). It does *not* modify your server code. That is your responsibility.

## Scenario 2: The realistic day-one mistake

Developers often start with: “I only need files + maybe a command or two”.

Try:

```bash
assay validate --config examples/demo/common-mistake/assay.yaml --format text
```

**Expected outcome:**
- `run_command` is allowed (high risk): suggests denying it or tightening allow/deny.
- `write_file` / `read_file` without path constraints: suggests adding constraints with `matches:` for safe roots.

Then preview fixes:

```bash
assay fix --config examples/demo/common-mistake/assay.yaml --dry-run
```

Apply fixes:

```bash
assay fix --config examples/demo/common-mistake/assay.yaml --yes
```
