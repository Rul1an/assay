# demo/explore.tape — TUI evidence explorer
# Note: requires assay built with --features tui and an evidence bundle
Output demo/output/explore.gif
Output demo/output/explore.mp4

Set FontSize 16
Set Width 1280
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 30ms
Set WindowBar Colorful
Set Margin 20
Set MarginFill "#1E1E2E"

# Setup: generate a bundle first
Hide
Type "export PS1='➜ '"
Enter
Type "export PATH=$PWD/target/debug:$PATH"
Enter
Type "cd demo/fixtures"
Enter
# Generate evidence bundle for explore
Type "assay run --config eval.yaml --trace-file traces/safe.jsonl 2>/dev/null"
Enter
Sleep 2s
Type "clear"
Enter
Show

Type "# 📊 Interactive evidence explorer"
Enter
Sleep 500ms

# Open TUI explorer on the generated bundle
Type "assay evidence explore --bundle bundle.tar.gz"
Enter
Sleep 3s

# Navigate with keyboard
Down
Sleep 500ms
Down
Sleep 500ms
Enter
Sleep 2s

# Exit
Type "q"
Sleep 1s

Screenshot demo/output/explore-thumb.png
