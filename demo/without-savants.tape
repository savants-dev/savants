# VHS tape: Claude Code WITHOUT Savants
# Install vhs: brew install charmbracelet/tap/vhs
# Run: vhs without-savants.tape

Output demo/without-savants.gif
Set Width 900
Set Height 500
Set FontSize 14
Set FontFamily "JetBrains Mono"
Set Theme "Catppuccin Mocha"
Set Padding 20

Type "# Claude Code without Savants"
Enter
Sleep 500ms
Type '# Prompt: "Where is the log level validation in Fastify?"'
Enter
Sleep 1s

Type "# Step 1: Claude greps for logLevel"
Enter
Sleep 500ms
Type 'grep -rn "logLevel" lib/ | head -8'
Enter
Sleep 2s

Type "# 14 matches. Which one validates? Claude reads files..."
Enter
Sleep 1s

Type "# Step 2: Read route.js around the matches"
Enter
Sleep 500ms
Type "sed -n '280,290p' lib/route.js"
Enter
Sleep 2s

Type "# Found where logLevel is SET, not where it's VALIDATED"
Enter
Sleep 1s

Type "# Step 3: Read more of route.js"
Enter
Sleep 500ms
Type "sed -n '460,470p' lib/route.js"
Enter
Sleep 2s

Type "# Still not found. This is where logLevel is USED at runtime."
Enter
Type "# The validation function is at line 641."
Enter
Type "# Claude would need more grep/read cycles to find it."
Enter
Sleep 2s

Type "# Total: 4+ tool calls, ~12,000 tokens, still searching"
Enter
Sleep 3s
