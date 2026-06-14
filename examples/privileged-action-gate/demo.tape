# VHS tape — the deterministic source for the privileged-action-gate demo GIF.
# Render: `vhs examples/privileged-action-gate/demo.tape` from the repo root (needs a built
# assay-mcp-server). The GIF is regenerated locally from this tape, not in CI — rendered GIFs vary
# by font/environment. CI instead smoke-tests the demo behaviour (verify.sh), so the hero cannot
# silently drift from the producer.
Output examples/privileged-action-gate/demo.gif

Set Shell bash
Set FontSize 15
Set Width 1180
Set Height 640
Set Padding 16
Set Theme "Dracula"

Hide
Type "cd examples/privileged-action-gate"
Enter
Show

Type "./run.sh"
Enter
Sleep 5s
