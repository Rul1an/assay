# Demo: Break & Fix

Deze demo laat zien hoe Assay **realistische** policy mistakes detecteert en hoe je naar een veilige policy toe werkt.

> Tip: begin met `enforcement.unconstrained_tools: warn` om gaps te zien zonder direct alles te breken.

---

## Scenario 1 — Te breed (unsafe allow-all)

Run:

```bash
assay validate --config examples/demo/unsafe-policy.yaml --format text
```

Expected:
- findings over tools die te veel permissies geven
- suggested patches / actions (afhankelijk van je setup)

---

## Scenario 2 — The realistic mistake (day-one-config)

Veel developers starten met "ik wil files lezen/schrijven en soms een command runnen".

Run:

```bash
assay validate --config examples/demo/common-mistake.yaml --format text
```

Expected (high-level):
- `run_command` zonder schema → `E_TOOL_UNCONSTRAINED` warning (of deny als je enforcement op deny zet)
- `write_file` zonder schema → idem
- geen schema’s = geen argument constraints (paden, patterns, etc.)

---

## Make it safe (v2 schemas)

Maak (of kopieer) een policy met schemas:

```yaml
version: "2.0"
name: "demo-safe"

tools:
  allow: ["read_file", "list_directory"]
  deny: ["run_command", "execute_*", "spawn*"]

schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/workspace/.*"
        minLength: 1
        maxLength: 4096
    required: ["path"]

  list_directory:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/workspace/.*"
        minLength: 1
        maxLength: 4096
    required: ["path"]

enforcement:
  unconstrained_tools: deny
```

Run:

```bash
assay validate --config /path/to/your-policy.yaml --format text
```

---

## Migrating legacy v1 policies

Preview:

```bash
assay policy migrate --input examples/demo/unsafe-policy.yaml --dry-run
```

Apply:

```bash
assay policy migrate --input examples/demo/unsafe-policy.yaml
```
