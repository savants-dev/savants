#!/bin/bash
# Savants Benchmark Runner
#
# Runs the full A/B comparison automatically.
# Records everything for reproducibility.
#
# Usage: ./benchmarks/run.sh [repo_path] [prompt]

set -e

REPO="${1:-/tmp/fastify}"
PROMPT="${2:-Find the function that validates log level options in this codebase. Show the exact file, function name, and line number.}"
RESULTS_DIR="/tmp/savants-benchmark-$(date +%Y%m%d-%H%M%S)"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

mkdir -p "$RESULTS_DIR"

echo "╔══════════════════════════════════════════════════╗"
echo "║          SAVANTS BENCHMARK                       ║"
echo "╠══════════════════════════════════════════════════╣"
echo "║  Repo:   $(basename $REPO)"
echo "║  Prompt: ${PROMPT:0:50}..."
echo "║  Output: $RESULTS_DIR"
echo "╚══════════════════════════════════════════════════╝"
echo ""

# Ensure repo is indexed
if command -v savants &>/dev/null; then
    echo "Indexing repo with savants..."
    savants reindex --repo-path "$REPO" 2>&1 | tail -2
    echo ""
fi

# Save the original MCP config
MCP_FILE="$REPO/.mcp.json"
[ -f "$MCP_FILE" ] && cp "$MCP_FILE" "$RESULTS_DIR/mcp-original.json"

# ── RUN 1: Without savants ──────────────────────────────
echo "━━━ RUN 1: WITHOUT SAVANTS ━━━"
echo ""
echo '{"mcpServers":{}}' > "$MCP_FILE"

START1=$(date +%s%3N)
claude -p "$PROMPT" --output-format json 2>/dev/null > "$RESULTS_DIR/without-savants.json" || true
END1=$(date +%s%3N)
WALL1=$((END1 - START1))

echo "  Wall time: ${WALL1}ms"
python3 -c "
import json
with open('$RESULTS_DIR/without-savants.json') as f: d = json.loads(f.read())
u = d.get('usage',{})
print(f'  Tokens:   {u.get(\"input_tokens\",0)+u.get(\"output_tokens\",0):,}')
print(f'  Turns:    {d.get(\"num_turns\",0)}')
print(f'  Answer:   {d.get(\"result\",\"\")[:100]}')
" 2>/dev/null || echo "  (parsing failed)"

echo ""

# ── RUN 2: With savants ─────────────────────────────────
echo "━━━ RUN 2: WITH SAVANTS ━━━"
echo ""

SAVANTS_BIN=$(command -v savants || echo "$HOME/.savants/bin/savants")
cat > "$MCP_FILE" << MCPEOF
{"mcpServers":{"savants":{"command":"$SAVANTS_BIN","args":["serve"]}}}
MCPEOF

START2=$(date +%s%3N)
claude -p "$PROMPT" --output-format json 2>/dev/null > "$RESULTS_DIR/with-savants.json" || true
END2=$(date +%s%3N)
WALL2=$((END2 - START2))

echo "  Wall time: ${WALL2}ms"
python3 -c "
import json
with open('$RESULTS_DIR/with-savants.json') as f: d = json.loads(f.read())
u = d.get('usage',{})
print(f'  Tokens:   {u.get(\"input_tokens\",0)+u.get(\"output_tokens\",0):,}')
print(f'  Turns:    {d.get(\"num_turns\",0)}')
print(f'  Answer:   {d.get(\"result\",\"\")[:100]}')
" 2>/dev/null || echo "  (parsing failed)"

echo ""

# ── Compare ──────────────────────────────────────────────
python3 "$SCRIPT_DIR/compare.py" "$RESULTS_DIR/without-savants.json" "$RESULTS_DIR/with-savants.json"

# Restore original MCP config
[ -f "$RESULTS_DIR/mcp-original.json" ] && cp "$RESULTS_DIR/mcp-original.json" "$MCP_FILE"

# Save metadata
cat > "$RESULTS_DIR/metadata.json" << METAEOF
{
    "repo": "$REPO",
    "prompt": "$PROMPT",
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "savants_version": "$(savants --version 2>/dev/null || echo unknown)",
    "claude_version": "$(claude --version 2>/dev/null || echo unknown)",
    "os": "$(uname -s) $(uname -m)",
    "wall_time_without_ms": $WALL1,
    "wall_time_with_ms": $WALL2
}
METAEOF

echo ""
echo "Full results saved to: $RESULTS_DIR/"
echo "Share: https://github.com/savants-dev/savants/issues/new?title=Benchmark+Result"
