# Perspective — Agent Guidelines

## Project Overview
Perspective is a graph+vector memory engine for AI agents, written in Rust.
MIT license. Standalone engine with first-class Hermes integration.

## Architecture
- Workspace with 3 crates: perspective-core, perspective-server, perspective-plugin
- Storage: Qdrant (vectors) + redb (graph) + Tantivy (BM25)
- Memory types: episodic, semantic, procedural
- LLM extraction via generic OpenAI-compatible API
- Ebbinghaus decay, periodic consolidation

## Build Commands
- `cargo check` — verify compilation
- `cargo test` — run tests
- `cargo build` — full build
- `cargo clippy` — lint

## Code Style
- Rust 2021 edition
- `thiserror` for error types
- `serde` for serialization
- `async-trait` for async traits
- `tracing` for logging
- No `unwrap()` in production code, use `?` or `map_err`

## Testing
- Unit tests in each module
- Integration tests in `tests/`
- Use `#[tokio::test]` for async tests

## Key Files
- `crates/perspective-core/src/types/` — Memory type definitions
- `crates/perspective-core/src/engine.rs` — Main engine struct
- `crates/perspective-core/src/store/` — Storage layer
- `crates/perspective-core/src/retrieval/` — Retrieval pipeline
- `crates/perspective-plugin/` — Hermes integration
- `ARCHITECTURE.md` — Full architecture document
