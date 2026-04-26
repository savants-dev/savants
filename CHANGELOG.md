# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-04-26

### Added
- `semantic_search` - find code by concept using ONNX embeddings (90% accuracy, 290ms cached)
- `file_skeleton` - file structure without bodies (28x compression vs reading the file)
- `callers` - find all functions that call a given function (11ms)
- `where_used` - find all callers and importers of a symbol (11ms)
- `reindex` - parse and index a repository with tree-sitter + ONNX embeddings
- MCP server for Claude Code, Cursor, Windsurf, and any MCP client
- Cloud proxy mode for savants.cloud integration
- TypeScript, JavaScript, Python, and Rust language support
- Local embedding cache at `~/.savants/embeddings/` for instant searches
- Local call index at `~/.savants/calls/` for caller/importer lookups
- `savants up` - detect environment and show status
- `savants status` - show connection and index status
- `savants connect` - authenticate with savants.cloud
- `savants usage` - show cloud usage statistics
