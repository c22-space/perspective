# Perspective — Agent Guidelines

## Project Overview
Perspective is a graph+vector memory engine for AI agents, written in Rust.
MIT license. Standalone engine with first-class Hermes integration.

## Architecture
- - Workspace with 4 crates: perspective-core, perspective-cli (CLI), perspective-plugin, perspective-python
- HTTP server lives in perspective-core, auto-starts on port 2085
- Storage: Qdrant-edge (embedded vectors) + redb (graph) + Tantivy (BM25)
- Memory types: episodic, semantic, procedural
- LLM extraction: bundled model (NuExtract via llama-cpp-2) or external OpenAI-compatible API
- Ebbinghaus decay, periodic consolidation

## Build Commands
- `cargo check` — verify compilation (first run takes ~4 min, compiles llama.cpp)
- `cargo test` — run tests
- `cargo build` — full build
- `cargo clippy` — lint

### Build Dependencies
- `libclang-dev` and `cmake` required for llama-cpp-2 (compiles llama.cpp from source)

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
- `crates/perspective-core/src/server.rs` — HTTP server (auto-starts on :2085)
- `crates/perspective-core/src/static_files.rs` — Dashboard static file serving
- `crates/perspective-cli/src/main.rs` — CLI only (init, status, config)
- `crates/perspective-core/src/types/` — Memory type definitions (memory.rs, graph.rs)
- `crates/perspective-core/src/engine.rs` — Main engine struct
- `crates/perspective-core/src/store/` — Storage layer (vector.rs, graph.rs, text.rs)
- `crates/perspective-core/src/retrieval/` — Retrieval pipeline (vector, graph, text, entity search + fusion)
- `crates/perspective-core/src/llm.rs` — Bundled LLM (llama-cpp-2) wrapper
- `crates/perspective-core/src/extraction/pipeline.rs` — Extraction routing (bundled vs HTTP)
- `crates/perspective-plugin/` — Hermes integration
- `crates/perspective-python/` — Python bindings (PyO3)
- `ARCHITECTURE.md` — Full architecture document
