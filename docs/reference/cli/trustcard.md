# assay trustcard

Generate Trust Card artifacts from a verified evidence bundle.

---

## Synopsis

```bash
assay trustcard <COMMAND> [OPTIONS]
```

---

## Generate

Generate `trustcard.json`, `trustcard.md`, and `trustcard.html`:

```bash
assay trustcard generate evidence.tar.gz --out-dir trustcard
```

The Trust Card is a one-way projection of Trust Basis claim rows plus frozen
non-goals:

- `trustcard.json` is the canonical Trust Card artifact.
- `trustcard.md` is a deterministic Markdown projection for text review.
- `trustcard.html` is a deterministic single-file static HTML projection for
  browser review.

The HTML artifact has inline styles only plus a static-page Content Security
Policy. It does not require JavaScript, remote assets, a hosted backend, or
network access. It includes accessible table structure, responsive overflow,
dark-mode, forced-colors, and print styling, but it does not add claim
semantics, scores, badges, or a second classifier.

---

## Options

| Option | Meaning |
|---|---|
| `<BUNDLE>` | Evidence bundle archive (`.tar.gz`). |
| `--out-dir <DIR>` | Directory that receives `trustcard.json`, `trustcard.md`, and `trustcard.html`. |
| `--pack <PACK[,PACK...]>` | Optional pack references to execute while classifying pack findings. |
| `--max-results <N>` | Maximum lint results considered when pack execution is enabled. Default: `500`. |

---

## Contract

Trust Card rendering must stay a projection layer. Claim classification happens
in [`assay trust-basis`](./trust-basis.md), and consumers should key claims by
stable `claim.id`, not by row position or count.

---

## See Also

- [Trust Basis CLI](./trust-basis.md)
- [Receipt family matrix](../receipt-family-matrix.json)
- [P52-P56 consolidation plan](../../architecture/PLAN-P52-P56-CONSOLIDATION-PROGRAM-2026q2.md)
