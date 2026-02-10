# demo/sim.tape — Attack simulation
# Note: requires assay built with --features sim
Output demo/output/sim.gif
Output demo/output/sim.mp4

Set FontSize 16
Set Width 1280
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 30ms
Set WindowBar Colorful
Set Margin 20
Set MarginFill "#1E1E2E"

# Setup
Hide
Type "export PS1='➜ '"
Enter
Type "export PATH=$PWD/target/debug:$PATH"
Enter
Type "cd demo/fixtures"
Enter
Type "clear"
Enter
Show

Type "# 🛡️ Attack simulation: test policy against known vectors"
Enter
Sleep 500ms

# Run sim with default suite
Type "assay sim run --config eval.yaml --policy policy.yaml"
Enter
Sleep 5s

Screenshot demo/output/sim-thumb.png
