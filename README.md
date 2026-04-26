<p align="center">
  <h1 align="center">savants</h1>
  <p align="center"><strong>Find code by describing what it does, not what it's called.</strong></p>
  <p align="center">90% accuracy. Under 400ms. Offline. No API keys.</p>
</p>

<p align="center">
  <a href="https://savants.dev">Website</a> ·
  <a href="#install">Install</a> ·
  <a href="#before--after">Before & After</a> ·
  <a href="https://savants.dev/case-study">Case Study</a>
</p>

---

## Before & After

> Tested on [Fastify](https://github.com/fastify/fastify) (33K stars, 287 files, 2,152 functions)

**Prompt to Claude Code:** *"Where is the log level validation in Fastify?"*

### Without Savants

Claude uses Grep and Read. 4 tool calls. ~12,000 tokens. Still doesn't find it.

```
> Grep "logLevel" lib/

lib/symbols.js:10   kLogLevel: Symbol('fastify.logLevel'),
lib/context.js:31   logLevel,
lib/context.js:67   this.logLevel = logLevel || server[kLogLevel]
lib/route.js:287    opts.logLevel = opts.logLevel || this[kLogLevel]
lib/route.js:347    logLevel: opts.logLevel,
lib/route.js:386    context.logLevel = opts.logLevel
lib/route.js:466    level: context.logLevel
... (14 matches)

> Read lib/route.js lines 280-290

    opts.logLevel = opts.logLevel || this[kLogLevel]

> Read lib/context.js lines 60-70

    this.logLevel = logLevel || server[kLogLevel]

> Read lib/route.js lines 460-470

    const loggerOpts = {
      level: context.logLevel
    }
```

**Result:** 4 tool calls. 12,000 tokens burned. Found where logLevel is *used* but not where it's *validated*. The validation function is at line 641 — Claude would need more grep/read cycles to find it.

### With Savants

One call. 400ms. Found it.

```
> semantic_search "log level validation"

lib/route.js:641       validateLogLevelOption()  [0.696]
lib/logger-factory.js:60  validateLogger()       [0.549]
lib/config-validator.js:10 validate10()          [0.440]
```

**`validateLogLevelOption` at line 641.** First result. 150 tokens.

Claude then asks for context:

```
> file_skeleton lib/route.js

24 functions:
  buildRouting() (line 76)
  prepareRoute() (line 146)
  addNewRoute() (line 280)
  routeHandler() (line 462)
  validateLogLevelOption() (line 641)
  ... 19 more

> callers validateLogLevelOption

Callers (1):
  addNewRoute (lib/route.js)
```

**3 tool calls. 450 tokens. Found the function, its location, the full file structure, and who calls it.**

### The numbers

| | Without Savants | With Savants |
|---|---|---|
| Tool calls | 4+ (and still searching) | 3 (done) |
| Tokens | ~12,000 | ~450 |
| Time | ~5 seconds | ~500ms |
| Found the right function | No | Yes |

This was a real test on real code. Fastify [issue #6124](https://github.com/fastify/fastify/issues/6124) reported that invalid log levels crash the server. Savants found `validateLogLevelOption` in one call. We [submitted the fix](https://github.com/fastify/fastify/pull/6683).

---

## Install

```bash
curl -fsSL savants.sh | sh
```

Then in your project:

```bash
savants reindex --repo-path .
```

Your code is parsed and indexed locally. Nothing leaves your machine.

## Use with Claude Code

Add to `~/.mcp.json`:

```json
{
  "mcpServers": {
    "savants": {
      "command": "savants",
      "args": ["serve"]
    }
  }
}
```

Restart Claude Code. Ask it anything about your codebase. Savants answers instead of grep.

## What You Get

| Tool | What it does | Speed |
|---|---|---|
| `semantic_search` | Find code by concept, not by name | ~400ms |
| `file_skeleton` | File structure without reading the full file | ~85ms |
| `callers` | Every function that calls a given function | ~15ms |
| `where_used` | All callers + importers of a symbol | ~15ms |
| `reindex` | Parse and index your repo | ~15s |

All tools work **offline**. No API keys. No cloud account. No data leaves your machine.

Index stays fresh automatically — Savants detects modified files and branch switches, re-indexes before answering.

## How It Works

`savants reindex` parses your code with [tree-sitter](https://tree-sitter.github.io/tree-sitter/) and builds two local indexes:

- **Embedding cache** — ONNX vectors for semantic search
- **Call index** — caller/callee relationships from parsed code

`savants serve` starts an MCP server. Your AI agent calls it instead of grep.

Supported: TypeScript, JavaScript, Python, Rust. More coming.

## Want More?

The free tools handle search and structure. For production intelligence:

```bash
savants connect
```

This unlocks additional cloud tools:

- **`diagnose-error`** — Root cause analysis across code, git, Slack, Jira, Sentry
- **`pr-risk`** — Pull request risk assessment
- **`blast_radius`** — What breaks if you change this function
- **`radar`** — What your team discussed while you were away
- [Full list →](https://savants.dev/#pricing)

## Works With

- **Claude Code** · **Cursor** · **Windsurf** · Any MCP client

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). We welcome new language support, search improvements, and bug fixes.

## License

[FSL-1.1-Apache-2.0](LICENSE) — Free to use, modify, and self-host. Cannot be offered as a competing hosted service. Converts to Apache 2.0 after two years.

---

<p align="center">
  Made by <a href="https://savants.dev">Savants</a>
</p>
