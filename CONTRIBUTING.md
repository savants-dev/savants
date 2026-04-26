# Contributing to Savants

Thank you for your interest in contributing to Savants! This guide will help you get started.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/savants.git`
3. Create a branch: `git checkout -b feat/your-feature`
4. Make your changes
5. Run the build: `cargo build --features embeddings`
6. Submit a pull request

## Development Setup

**Prerequisites:**
- Rust 1.82+
- OpenSSL development headers (`openssl-dev` or `libssl-dev`)

**Build:**
```bash
cargo build                      # without ONNX embeddings
cargo build --features embeddings  # with ONNX embeddings (recommended)
```

**Test:**
```bash
cargo test
```

## Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add Go language support
fix: handle symlinks in file walker
docs: add example for Cursor integration
perf: reduce reindex time by 40%
refactor: extract tokenizer into separate module
test: add accuracy benchmarks for semantic search
ci: add aarch64 build target
```

**Types:** `feat`, `fix`, `docs`, `perf`, `refactor`, `test`, `ci`, `chore`

## Pull Request Process

1. Update the README if you've changed public-facing behavior
2. Add tests for new features
3. Ensure CI passes
4. Use a descriptive title following conventional commits
5. Describe what changed and why in the PR body

## Adding Language Support

Savants uses [tree-sitter](https://tree-sitter.github.io/) for parsing. To add a new language:

1. Add the tree-sitter grammar crate to `Cargo.toml`
2. Add a new match arm in `code_parser.rs` for the file extension
3. Add the language to `parse_file()`
4. Test with a real repository in that language
5. Update the README's supported languages table

## Reporting Bugs

Use [GitHub Issues](https://github.com/savants-dev/savants/issues) with the bug report template. Include:
- Your OS and architecture
- Rust version (`rustc --version`)
- Steps to reproduce
- Expected vs actual behavior

## Feature Requests

Open a [GitHub Issue](https://github.com/savants-dev/savants/issues) with the feature request template. Describe the problem you're trying to solve, not just the solution you want.

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Keep functions focused and small
- Add doc comments for public APIs

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
