Output demo/scenes/ci-failure.mp4
Set FontSize 24
Set Width 1280
Set Height 720
Set Padding 40
Set Theme "Catppuccin Mocha"
Hide
Type "gh run watch"
Enter
Show
Type "✓ Set up job"
Sleep 200ms
Enter
Type "✓ Run actions/checkout@v4"
Sleep 200ms
Enter
Type "✓ Install Rust"
Sleep 500ms
Enter
Type "○ Run cargo test"
Sleep 500ms
Enter
Type "Running tests..."
Sleep 1s
Enter
Type "test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out"
Sleep 500ms
Enter
Type "Error: Process completed with exit code 101."
Sleep 500ms
Enter
Type "❌ Run failed with exit code 1"
Sleep 2s
