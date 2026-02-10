# demo/break-fix.tape — Break & Fix deep dive
Output demo/output/break-fix.gif
Output demo/output/break-fix.mp4

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

Type "# 🔴 Agent calls a denied tool"
Enter
Sleep 500ms

# 1. Unsafe Run -> FAIL
Type "assay run --config eval.yaml --trace-file traces/unsafe.jsonl"
Enter
Sleep 4s

Type "# 🟢 Fix: Use only allowed tools"
Enter
Sleep 1s

# 2. Safe Run -> PASS
Type "assay run --config eval.yaml --trace-file traces/safe.jsonl"
Enter
Sleep 3s

Screenshot demo/output/break-fix-thumb.png
