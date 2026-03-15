# ADR-032 Obsidian View Layer Recommendations (2026 Q2)

> Status: Recommended internal view-layer setup
> Scope: Obsidian usage for navigating ADR-032 and related architecture docs

This page records the recommended Obsidian setup for the ADR-032 documentation line.
The repo remains canonical; Obsidian is the internal navigation and insight layer.

## Principle

Use Obsidian to improve navigation, insight, and review speed.
Do not use it as a second source of truth.

## Recommended Core Plugins

### Bases
Recommended as the primary metadata-driven navigation layer.
Use it for note sets with explicit properties and lightweight dashboards.

Source:
- [Obsidian Bases](https://help.obsidian.md/bases)

### Canvas
Recommended for visual architecture navigation and mapping documents, notes, and web pages together.
Obsidian stores canvas data as `.canvas` files using the open JSON Canvas format.

Source:
- [Obsidian Canvas](https://help.obsidian.md/plugins/canvas)

### Graph view
Recommended for exploring note relationships and local neighborhoods when the note graph is curated and link-rich.
Useful as a secondary discovery tool, not the primary architecture index.

Source:
- [Obsidian Graph view](https://help.obsidian.md/plugins/graph)

### Web viewer
Recommended for reading canonical repo docs or web research inside Obsidian without context-switching.
Use it for convenience, not for sensitive browsing.

Sources:
- [Obsidian Web viewer](https://help.obsidian.md/plugins/web-viewer)
- [Obsidian plugin security](https://help.obsidian.md/plugin-security)

## Recommended Community Plugins

### Dataview
Recommended as the stable query/index engine when you need live views over note metadata.
Useful for architecture review dashboards and experiment status pages.

Source:
- [Dataview docs](https://blacksmithgu.github.io/obsidian-dataview/)

### Excalidraw
Recommended when architecture sketches need to live in the vault, embed into notes, and link to Markdown content.
Best for freeform review diagrams, not for replacing the canonical Structurizr model.

Source:
- [Obsidian Excalidraw plugin](https://github.com/zsviczian/obsidian-excalidraw-plugin)

## Bleeding-Edge / Opt-In

### Datacore
Recommended only for power users who want dynamic editable views and are comfortable with JavaScript/TypeScript-heavy workflows.
As of March 2026, Datacore describes itself as a power tool and says it is still in a power-user stage.

Source:
- [Datacore docs](https://blacksmithgu.github.io/datacore/)

## Security Guidance

Obsidian community plugins are powerful, but not sandboxed.
That means plugin choice should be conservative when the vault is used for sensitive internal material.

Practical rule:
- default to core plugins first
- add Dataview or Excalidraw when they clearly reduce friction
- treat Datacore as an explicit opt-in
- keep sensitive browsing in your primary browser instead of Web viewer

Sources:
- [Obsidian plugin security](https://help.obsidian.md/plugin-security)
- [Obsidian Web viewer](https://help.obsidian.md/plugins/web-viewer)

## Recommended Stack For ADR-032

### Baseline
- Bases
- Canvas
- Graph view
- Web viewer

### Stable augmentation
- Dataview
- Excalidraw

### Optional bleeding edge
- Datacore

## What Not To Do

- Do not make Obsidian notes the canonical architecture source.
- Do not rely on community plugins for security-sensitive workflows without review.
- Do not replace C4/Structurizr diagrams with only freeform canvas sketches.
