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
- **Built-in HTTP server** — Serves dashboard + REST API on port 2085

---

## Quick Start

### Prerequisites

- Rust 2021 edition (1.75+)
- Build deps for llama-cpp-2: `sudo apt install libclang-dev cmake`

No Docker. No external services. All storage is embedded.

### Build

```bash
cargo build --release -p perspective-cli
```

### Start the Engine

```bash
# Start as daemon
perspective start -d ~/.local/share/perspective

# Check health
curl http://127.0.0.1:2085/api/health

# Store a memory
curl -X POST http://127.0.0.1:2085/api/store \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"hermes","content":"remember this","memory_type":"episodic"}'

# Recall memories
curl -X POST http://127.0.0.1:2085/api/recall \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"hermes","query":"what should I remember","budget":5}'

# Stop
perspective stop
```

### Run Tests

```bash
cargo test
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
│   ├── perspective-cli/          # CLI tool (init, status, config, start, stop)
│   └── perspective-plugin/       # Hermes MemoryProvider plugin (Rust)
├── dashboard/                    # React + TypeScript dashboard
└── tests/
    └── integration/
```

---

## Crates

| Crate | Purpose |
|-------|---------|
| `perspective-core` | Core engine: types, storage, retrieval, extraction, decay, consolidation |
| `perspective-cli` | CLI binary: init, status, config, start, stop |
| `perspective-plugin` | Hermes integration via `MemoryProvider` trait |

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
npm ci
npm run build
```

Dashboard is served automatically when the engine starts on port 2085.

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
