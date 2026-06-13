# Reproducible demo GIF for the README hero.
# Render with charmbracelet/vhs:  vhs demo/mcp-wrap.tape
# Prereqs: `assay` on PATH (cargo install assay-cli) and npx available.
# Output is deterministic given the same assay version + policy fixture.

Output demo/output/mcp-wrap.gif

Set Shell "bash"
Set FontSize 18
Set Width 1100
Set Height 600
Set Padding 24
Set Theme "Github Dark"
Set TypingSpeed 35ms

Hide
Type "mkdir -p /tmp/assay-demo && echo 'safe content' > /tmp/assay-demo/safe.txt && clear"
Enter
Show

Sleep 600ms
Type "assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \"
Enter
Type "  -- npx @modelcontextprotocol/server-filesystem /tmp/assay-demo"
Enter
Sleep 2.5s

# The proxy decides each tools/call before it runs (fail-closed, with the reason),
# and writes an offline-verifiable evidence bundle. Let the allow/deny lines settle.
Sleep 3s
