# demo/evidence-lint.tape — Policy validation
Output demo/output/evidence-lint.gif
Output demo/output/evidence-lint.mp4

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

# Validate unsafe trace → shows violations
Type "assay validate --config eval.yaml --trace-file traces/unsafe.jsonl"
Enter
Sleep 4s

# Validate safe trace → clean
Type "assay validate --config eval.yaml --trace-file traces/safe.jsonl"
Enter
Sleep 3s

Screenshot demo/output/evidence-lint-thumb.png
