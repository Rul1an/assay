# demo/full-walkthrough.tape — Complete golden-path demo
Output demo/output/full-walkthrough.gif
Output demo/output/full-walkthrough.mp4

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
Type "rm -rf demo/walkthrough_tmp && mkdir -p demo/walkthrough_tmp && cp -r demo/fixtures/* demo/walkthrough_tmp/ && cd demo/walkthrough_tmp"
Enter
Type "clear"
Enter
Show

# 1. Initialize
Type "assay init --hello-trace --ci github"
Enter
Sleep 2s

# 2. Run Policy Check (Safe)
Type "assay run --config eval.yaml --trace-file traces/safe.jsonl"
Enter
Sleep 3s

# 3. Validation (Unsafe)
Type "assay validate --config eval.yaml --trace-file traces/unsafe.jsonl"
Enter
Sleep 3s

Screenshot demo/output/full-walkthrough-thumb.png
