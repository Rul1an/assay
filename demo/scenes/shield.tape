Output demo/scenes/shield.mp4
Set FontSize 18
Set Width 1280
Set Height 720
Set Padding 40
Set Theme "Catppuccin Mocha"
Hide
Type "alias assay='$PWD/demo/mocks/assay-mock.sh'"
Enter
Type "clear"
Show
Type "assay sim run --target prod.bundle --suite chaos"
Show
Type "assay sim run --target prod.bundle --suite chaos"
Enter
Sleep 5s
