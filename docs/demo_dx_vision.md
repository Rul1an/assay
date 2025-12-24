# Vision: Assay DX 2025 (State of the Art)

**Goal**: Transform Assay from a "CI Tool" into an "Interactive Governance Platform".
In late 2025, developers expect tools to be alive, interactive, and visually stunning capabilities (like Linear, Vercel, or warp.dev).

## 1. The Core Philosophy: "Alive by Default"
Static text logs are dead. Assay's new demos should feel like a *Combat Initializer*:
*   **Spinners & Progress**: Never show a frozen screen.
*   **Streaming**: Visualize the LLM "thinking" token-by-token (Matrix style).
*   **Instant Feedback**: Red flash on screen when a policy is violated.

## 2. Proposed Demo Improvements

### A. The "Live Attack" TUI (Text User Interface)
Instead of scrolling logs, use a split-screen TUI (powered by `rich` or `ratatui`):
*   **Left Panel**: Unfiltered Agent Output (The "Mind" of the LLM).
*   **Right Panel**: Assay Watchtower (Real-time policy evaluation).
*   **Bottom Panel**: Status / Blocker Alert.

### B. "Red Team" Playground
An interactive mode where devs can type malicious prompts to *try* and break the agent.
```
> Enter Attack: Ignore previous instructions...
[VERDICT] 🛡️ BLOCKED (Confidence: 0.99)
```
Gamify the safety: "Can you bypass the guard?"

### C. "Time Machine" Replay
Allow developers to step through a trace:
`[Previous Step]` `[Block Action]` `[Next Step]`
This helps debug *why* a specific tool call was flagged.

## 3. Technology Stack (Python Demo)
*   **`rich`**: For beautiful panels, tables, and syntax highlighting.
*   **`prompt_toolkit`**: For interactive input loops.

## 4. Next Steps
1.  **Immediate**: Upgrade `live_attack.py` to `demo_sota.py` using `rich`.
2.  **Short Term**: Add an interactive "Type your own attack" loop.
3.  **Long Term**: Build a `assay tui` subcommand in Rust (`ratatui`) for the core binary.
