# VHS tape file for recording devpulse demo GIF
# Install vhs: brew install charmbracelet/tap/vhs
# Run: vhs demo.tape

Output demo.gif

Set FontSize 14
Set Width 1200
Set Height 600
Set Theme "Dracula"
Set Padding 20

Type "devpulse ~/projects"
Sleep 500ms
Enter
Sleep 3s

Type "devpulse --filter dirty --sort name ~/projects"
Sleep 500ms
Enter
Sleep 3s

Type "devpulse --format json ~/projects | head -20"
Sleep 500ms
Enter
Sleep 2s
