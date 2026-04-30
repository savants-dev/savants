# Savants Benchmark: Reproducible A/B Comparison

**Anyone can reproduce this.** No proprietary data, no special setup.

## The Claim

Savants finds code **174x faster** than Claude Code without it.

| | Without Savants | With Savants |
|---|---|---|
| Time | 58+ minutes | 20 seconds |
| Tokens | tens of thousands | 503 |
| Result | Still searching | Found: `validateLogLevelOption` at `lib/route.js:641` |

## Reproduce It Yourself

### Prerequisites
- Claude Code (`npm install -g @anthropic-ai/claude-code` or via installer)
- Savants (`curl -fsSL savants.sh | sh`)

### Step 1: Clone and index the test repo

```bash
git clone --depth 1 https://github.com/fastify/fastify.git /tmp/fastify
savants reindex --repo-path /tmp/fastify
```

### Step 2: Run WITHOUT savants

Remove savants from your MCP config (or use a fresh `.mcp.json`):

```bash
echo '{"mcpServers":{}}' > /tmp/fastify/.mcp.json
cd /tmp/fastify
time claude -p "Find the function that validates log level options in this codebase. Show the exact file, function name, and line number." --output-format json > /tmp/without-savants.json
```

### Step 3: Run WITH savants

```bash
echo '{"mcpServers":{"savants":{"command":"savants","args":["serve"]}}}' > /tmp/fastify/.mcp.json
cd /tmp/fastify
time claude -p "Find the function that validates log level options in this codebase. Show the exact file, function name, and line number." --output-format json > /tmp/with-savants.json
```

### Step 4: Compare

```bash
python3 benchmarks/compare.py /tmp/without-savants.json /tmp/with-savants.json
```

## Why This Happens

Without savants, Claude Code:
1. Runs `grep -rn "validate" lib/` → 105 matches
2. Reads files one by one looking for the right function
3. Runs more greps with different patterns
4. Reads more files
5. Repeats until it finds it (or times out)

With savants, Claude Code:
1. Calls `semantic_search("function that validates log level options")`
2. Gets `validateLogLevelOption` at `lib/route.js:641` in 400ms
3. Done.

Savants uses ONNX embeddings to search by **meaning**, not text. It knows what "validates log level options" means and matches it to the function that does that, regardless of variable names.

## Test on Your Own Codebase

```bash
savants reindex --repo-path /path/to/your/repo
savants search "your natural language query"
```

## Methodology

- **Repo**: [Fastify](https://github.com/fastify/fastify) (33K stars, 287 files, 2,152 functions)
- **Task**: Find a specific function using a natural language description
- **Claude model**: Claude Opus 4.6 (same model for both runs)
- **Hardware**: Same machine, sequential runs
- **No caching**: Fresh Claude Code sessions for each run
- **Reproducible**: Public repo, public tool, documented steps

## Submit Your Results

Run the benchmark and share your results:

```bash
./benchmarks/run.sh | tee my-results.txt
```

Post your results as a [GitHub Issue](https://github.com/savants-dev/savants/issues/new?title=Benchmark+Result&template=benchmark.md) or tweet with #SavantsBenchmark.
