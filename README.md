# Perspective

**A memory engine for AI agents.** Graph + vector hybrid. Typed memory. LLM-powered extraction. Built in Rust.

---

## What is Perspective?

Perspective is a standalone memory engine that gives AI agents persistent, structured memory. It combines vector search (semantic similarity) with a knowledge graph (relationships, entities, causality) to provide retrieval that understands both *what* was said and *how it connects* to everything else.

### Memory Types

- **Episodic** — Specific events with temporal context. Decays fast, promotes to semantic when accessed frequently.
- **Semantic** — Extracted facts and knowledge. Decays slowly, sourced from episodic memories.
- **Procedural** — Skills and action patterns. Never decays, versioned and refined over time.

### Key Features
- **Hybrid retrieval** — Vector similarity + graph traversal + entity search, fused via RRF
- **Ebbinghaus decay** — Memories fade unless accessed. Prevents unbounded accumulation.
- **LLM extraction** — Bundled local model (NuExtract) or external API. Smart batching and importance gating keep costs low.
- **Consolidation** — Automatic deduplication, community detection, episodic-to-semantic promotion
- **Multi-tenant** — Collection-based isolation for multiple agents/users
- **Dual mode** — Embedded library or client-server (gRPC)

---

## Quick Start
### Prerequisites

- Rust 2021 edition (1.75+)
- Build deps for llama-cpp-2: `sudo apt install libclang-dev cmake`

No Docker. No external services. All storage is embedded.

### Build

```bash
cargo build
```

### Run Tests

```bash
cargo test
```

### Run the Server

```bash
cargo run -p perspective-server
```

---

## Project Structure

```
perspective/
├── Cargo.toml                    # Workspace root
├── ARCHITECTURE.md               # Full architecture doc
├── AGENTS.md                     # Agent guidelines
├── crates/
│   ├── perspective-core/         # Core engine library
│   ├── perspective-server/       # gRPC server + dashboard
│   ├── perspective-plugin/       # Hermes MemoryProvider plugin
│   └── perspective-python/       # Python bindings
├── dashboard/                    # React + TypeScript dashboard
└── tests/
    └── integration/
```

---

## Crates

| Crate | Purpose |
|-------|---------|
| `perspective-core` | Core engine: types, storage, retrieval, extraction, decay, consolidation |
| `perspective-server` | gRPC server with health checks and embedded dashboard |
| `perspective-plugin` | Hermes integration via `MemoryProvider` trait |
| `perspective-python` | Python bindings (PyO3) |

---

## Storage
- **Qdrant-edge** — Embedded vector storage (no Docker required)
- **redb** — Embedded graph store (relationships, entities, metadata)
- **Tantivy** — BM25 full-text search

---

## Dashboard

The `dashboard/` directory contains a React + TypeScript + Vite app for monitoring and exploring memory.

```bash
cd dashboard
npm install
npm run dev
```

---

## Bundled LLM

Perspective bundles a local LLM for fact extraction. No external LLM server needed.

- **Model**: NuExtract-tiny-v1.5-Q5_K_M (401MB GGUF)
- **Runtime**: llama-cpp-2 (compiles llama.cpp from source)
- **Build deps**: `sudo apt install libclang-dev cmake`
- **First build**: `cargo check` takes ~4 min (compiles llama.cpp). Subsequent builds are cached.
- **Config**: `extraction.endpoint = ""` (empty) uses bundled model. Set to a URL for external HTTP mode.

---

## License

MIT — see [LICENSE](LICENSE) for details.
