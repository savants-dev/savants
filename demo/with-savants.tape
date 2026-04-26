# VHS tape: Claude Code WITH Savants
# Install vhs: brew install charmbracelet/tap/vhs
# Run: vhs with-savants.tape

Output demo/with-savants.gif
Set Width 900
Set Height 500
Set FontSize 14
Set FontFamily "JetBrains Mono"
Set Theme "Catppuccin Mocha"
Set Padding 20

Type "# Claude Code with Savants"
Enter
Sleep 500ms
Type '# Same prompt: "Where is the log level validation in Fastify?"'
Enter
Sleep 1s

Type "# Step 1: Claude calls semantic_search"
Enter
Sleep 500ms
Type 'savants semantic_search "log level validation"'
Enter
Sleep 1s

Type "=== Semantic search (5 results) ==="
Enter
Sleep 200ms
Type "  lib/route.js:641       validateLogLevelOption()  [0.696]"
Enter
Sleep 200ms
Type "  lib/logger-factory.js:60  validateLogger()       [0.549]"
Enter
Sleep 200ms
Type "  lib/config-validator.js:10 validate10()          [0.440]"
Enter
Sleep 2s

Type "# Found it. Line 641. First result. 400ms."
Enter
Sleep 1s

Type "# Step 2: Claude gets the file structure"
Enter
Sleep 500ms
Type 'savants file_skeleton "lib/route.js"'
Enter
Sleep 500ms

Type "24 functions:"
Enter
Type "  buildRouting() (line 76)"
Enter
Type "  addNewRoute() (line 280)"
Enter
Type "  routeHandler() (line 462)"
Enter
Type "  validateLogLevelOption() (line 641)"
Enter
Type "  ... 20 more"
Enter
Sleep 2s

Type "# Step 3: Who calls this function?"
Enter
Sleep 500ms
Type 'savants callers "validateLogLevelOption"'
Enter
Sleep 500ms

Type "Callers (1):"
Enter
Type "  addNewRoute (lib/route.js)"
Enter
Sleep 2s

Type "# Total: 3 calls, ~450 tokens, done."
Enter
Sleep 3s
