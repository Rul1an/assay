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
