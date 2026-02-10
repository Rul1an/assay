# demo/hero.tape — "Break & Fix" hero (value-first, no setup)
Output demo/output/hero.gif
Output demo/output/hero.mp4

Set FontSize 16
Set Width 1280
Set Height 720
Set Theme "Catppuccin Mocha"
Set TypingSpeed 30ms
Set WindowBar Colorful
Set Margin 20
Set MarginFill "#1E1E2E"

# Setup prompt
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

# 1. Unsafe → FAIL (tension in 2 seconds)
Type "assay run --config eval.yaml --trace-file traces/unsafe.jsonl"
Enter
Sleep 3s

# 2. Safe → PASS (resolution)
Type "assay run --config eval.yaml --trace-file traces/safe.jsonl"
Enter
Sleep 3s

Screenshot demo/output/hero-thumb.png
