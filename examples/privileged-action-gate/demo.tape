# VHS tape — the deterministic source for the privileged-action-gate demo GIF.
# Render: `vhs examples/privileged-action-gate/demo.tape` from the repo root (needs a built
# assay-mcp-server). CI regenerates and smoke-tests the render; the GIF itself may vary by
# font/environment, so CI verifies the render runs and that verify.sh passes, not byte-equality.
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
