# Perspective: Match Reality with Documentation

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Wire the disconnected systems (decay, consolidation, memory types) into the engine, kill gRPC, and clean up retrieval duplication so the docs accurately describe what the code does.

**Architecture:** The storage layer (qdrant-edge + redb + tantivy) and retrieval pipeline are fully functional. The gaps are: (1) decay math exists but is never called, (2) consolidation functions exist but are never called, (3) recall always reconstructs as Episodic regardless of type, (4) gRPC is dead deps with misleading banner, (5) retrieval module has unused standalone functions that recall() duplicates inline.

**Tech Stack:** Rust, qdrant-edge, redb, petgraph, tantivy, fastembed, llama-cpp-2

---

## Phase 1: Kill gRPC (10 min)

tonic/prost are dead dependencies. The server is HTTP only. The banner at main.rs:593 says "gRPC server" but nothing listens on that port.

### Task 1.1: Remove gRPC deps from server Cargo.toml

**Objective:** Remove tonic and prost from perspective-server dependencies.

**Files:**
- Modify: `crates/perspective-server/Cargo.toml:14-15`

**Step 1: Remove deps**

Remove these lines from `[dependencies]`:
```toml
tonic = "0.12"
prost = "0.13"
```

**Step 2: Verify build**

Run: `cargo check -p perspective-server`
Expected: compiles without errors (tonic/prost were never used in code)

**Step 3: Commit**

```bash
git add crates/perspective-server/Cargo.toml
git commit -m "chore: remove unused tonic/prost gRPC dependencies"
```

---

### Task 1.2: Fix misleading banner in server

**Objective:** Remove "gRPC server" line from serve command banner.

**Files:**
- Modify: `crates/perspective-server/src/main.rs:593`

**Step 1: Remove the gRPC banner line**

Change:
```rust
println!("  gRPC server:  {host}:{port}");
```
To: delete this line entirely.

Also remove `port` from the `Commands::Serve` struct args (it's accepted but never used):
- Modify: `crates/perspective-server/src/main.rs` — remove the `port` field from the `Serve` variant of the `Commands` enum, and remove `port` from the match destructuring.

**Step 2: Verify build**

Run: `cargo check -p perspective-server`
Expected: compiles, no warnings about unused `port`

**Step 3: Commit**

```bash
git add crates/perspective-server/src/main.rs crates/perspective-server/Cargo.toml
git commit -m "fix: remove misleading gRPC banner and unused port arg from serve command"
```

---

## Phase 2: Wire Decay into Engine (30 min)

The decay math (`ebbinghaus.rs`), maintenance functions (`maintenance.rs`), and scoring (`scorer.rs`) all exist with tests. They just need to be called from the engine.

### Task 2.1: Apply decay strength during recall

**Objective:** Filter recall results by decay strength so weak memories are excluded.

**Files:**
- Modify: `crates/perspective-core/src/engine.rs` (recall method, after loading memories ~line 501-548)

**Step 1: Import decay maintenance**

Add to engine.rs imports:
```rust
use crate::decay::maintenance::memory_strength;
```

**Step 2: Filter by strength after loading memories**

After the loop that loads full memories (line 501-548), add filtering:

```rust
// Filter out memories below retrieval threshold
let min_strength = self.config.decay.retrieval_threshold;
memories.retain(|m| {
    let strength = memory_strength(m);
    strength >= min_strength
});
```

**Step 3: Update scores to include strength**

When building `result_scores`, multiply the RRF score by the decay strength:

```rust
// In the loop building result_scores (line 547):
let strength = memory_strength(&memories.last().unwrap());
result_scores.push(score * strength);
```

**Step 4: Verify build**

Run: `cargo check -p perspective-core`
Expected: compiles

**Step 5: Commit**

```bash
git add crates/perspective-core/src/engine.rs
git commit -m "feat: apply Ebbinghaus decay strength filtering in recall()"
```

---

### Task 2.2: Reinforce memories on recall access

**Objective:** When a memory is recalled, update its access count and stability.

**Files:**
- Modify: `crates/perspective-core/src/engine.rs` (recall method, after loading memories)

**Step 1: Import ebbinghaus reinforcement**

Add to engine.rs imports:
```rust
use crate::decay::ebbinghaus::reinforce;
```

**Step 2: Reinforce accessed memories**

After loading memories in recall, before returning:

```rust
// Reinforce accessed memories (update stability based on access count)
for memory in &mut memories {
    match memory {
        Memory::Episodic(ref mut m) => {
            m.access_count += 1;
            m.last_accessed = Utc::now();
            m.stability = reinforce(m.stability, m.access_count, self.config.decay.learning_rate);
        }
        Memory::Semantic(ref mut m) => {
            m.access_count += 1;
            m.last_accessed = Utc::now();
            m.stability = reinforce(m.stability, m.access_count, self.config.decay.learning_rate);
        }
        Memory::Procedural(ref mut m) => {
            m.access_count += 1;
            m.last_used = Utc::now();
            // Procedural stability doesn't change (lambda = 0)
        }
    }
}
```

Note: This updates in-memory only. The vector store payload isn't updated (that would require re-indexing). The next store() of the same content would pick up the new values. For now, this is acceptable - the important thing is that recall() applies decay filtering.

**Step 3: Verify build**

Run: `cargo check -p perspective-core`
Expected: compiles

**Step 4: Commit**

```bash
git add crates/perspective-core/src/engine.rs
git commit -m "feat: reinforce memory stability on recall access"
```

---

### Task 2.3: Add background decay loop to engine

**Objective:** Periodically apply decay to all memories and flag GC candidates.

**Files:**
- Modify: `crates/perspective-core/src/engine.rs` (add `start_decay_loop` method)

**Step 1: Add the decay loop method**

```rust
/// Start a background loop that applies decay every `decay_interval_secs`.
/// Returns a JoinHandle for the background task.
pub fn start_decay_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
    let interval = std::time::Duration::from_secs(
        // Use a reasonable default: check every hour
        3600
    );
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if !self.config.decay.enabled {
                continue;
            }
            // List all tenants and apply decay to each
            if let Ok(tenants) = self.list_tenants().await {
                for tenant_id in &tenants {
                    if let Ok(memories) = self.list_memories(tenant_id, 10000).await {
                        let _results = crate::decay::maintenance::apply_decay_to_memories(
                            &memories,
                            &self.config.decay,
                        );
                        // Decay is computed on-the-fly during recall anyway.
                        // This loop exists to potentially trigger GC in the future.
                        tracing::debug!(
                            "Decay loop: processed {} memories for tenant {}",
                            _results.len(),
                            tenant_id
                        );
                    }
                }
            }
        }
    })
}
```

**Step 2: Start decay loop in server**

In `crates/perspective-server/src/main.rs`, after the extraction loop start (line 602-610), add:

```rust
// Start decay loop
{
    let decay_handle = engine.clone().start_decay_loop();
    tokio::spawn(async move {
        let _ = decay_handle.await;
    });
    println!("  ✓ Decay loop started (hourly)");
}
```

**Step 3: Verify build**

Run: `cargo check`
Expected: compiles

**Step 4: Commit**

```bash
git add crates/perspective-core/src/engine.rs crates/perspective-server/src/main.rs
git commit -m "feat: add background decay loop to engine and server"
```

---

## Phase 3: Wire Consolidation into Engine (30 min)

Consolidation functions (dedup, promotion, communities) exist with tests. They need to be called from the engine on a schedule.

### Task 3.1: Add consolidation method to engine

**Objective:** Create a `run_consolidation` method that calls dedup, promotion, and community detection.

**Files:**
- Modify: `crates/perspective-core/src/engine.rs` (add method)

**Step 1: Add imports**

```rust
use crate::consolidation::dedup::find_duplicates;
use crate::consolidation::promotion::find_promotable;
use crate::consolidation::communities::detect_communities;
```

**Step 2: Add the consolidation method**

```rust
/// Run a full consolidation cycle: dedup, promotion, community detection.
pub async fn run_consolidation(&self, tenant_id: &str) -> Result<ConsolidationReport> {
    let memories = self.list_memories(tenant_id, 10000).await?;
    let mut report = ConsolidationReport::default();

    // Phase 1: Deduplication
    if self.config.consolidation.enabled {
        let threshold = self.config.consolidation.dedup_similarity_threshold;
        let duplicates = find_duplicates(&memories, threshold);
        report.duplicates_found = duplicates.len();
        // TODO: actually merge duplicates (keep richer version, update graph edges)
        // For now, just report what would be merged
    }

    // Phase 2: Promotion (episodic -> semantic)
    if self.config.consolidation.enabled {
        let threshold = self.config.consolidation.promotion_access_count;
        let promotable = find_promotable(&memories, threshold);
        report.promotable_count = promotable.len();
        // TODO: actually create semantic memories from promoted episodic ones
        // For now, just report what would be promoted
    }

    // Phase 3: Community detection
    if let Some(ref gs) = self.graph_store {
        if let Ok(graph) = gs.load_graph(tenant_id) {
            let communities = detect_communities(&graph);
            report.communities = communities.len();
        }
    }

    Ok(report)
}
```

**Step 3: Add ConsolidationReport struct**

```rust
#[derive(Debug, Default, Clone)]
pub struct ConsolidationReport {
    pub duplicates_found: usize,
    pub promotable_count: usize,
    pub communities: usize,
}
```

**Step 4: Verify build**

Run: `cargo check -p perspective-core`
Expected: compiles

**Step 5: Commit**

```bash
git add crates/perspective-core/src/engine.rs
git commit -m "feat: add run_consolidation method to engine"
```

---

### Task 3.2: Add consolidation background loop

**Objective:** Run consolidation periodically (default: every 4 hours).

**Files:**
- Modify: `crates/perspective-core/src/engine.rs` (add `start_consolidation_loop`)

**Step 1: Add the loop method**

```rust
/// Start a background loop that runs consolidation every `interval_secs`.
pub fn start_consolidation_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
    let interval = std::time::Duration::from_secs(self.config.consolidation.interval_secs);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if !self.config.consolidation.enabled {
                continue;
            }
            if let Ok(tenants) = self.list_tenants().await {
                for tenant_id in &tenants {
                    match self.run_consolidation(tenant_id).await {
                        Ok(report) => {
                            tracing::info!(
                                "Consolidation for {}: {} duplicates, {} promotable, {} communities",
                                tenant_id,
                                report.duplicates_found,
                                report.promotable_count,
                                report.communities
                            );
                        }
                        Err(e) => {
                            tracing::error!("Consolidation failed for {}: {}", tenant_id, e);
                        }
                    }
                }
            }
        }
    })
}
```

**Step 2: Start consolidation loop in server**

In `crates/perspective-server/src/main.rs`, after the decay loop:

```rust
// Start consolidation loop
{
    let consolidation_handle = engine.clone().start_consolidation_loop();
    tokio::spawn(async move {
        let _ = consolidation_handle.await;
    });
    println!("  ✓ Consolidation loop started (every {}s)", config.consolidation.interval_secs);
}
```

**Step 3: Verify build**

Run: `cargo check`
Expected: compiles

**Step 4: Commit**

```bash
git add crates/perspective-core/src/engine.rs crates/perspective-server/src/main.rs
git commit -m "feat: add background consolidation loop to engine and server"
```

---

### Task 3.3: Wire consolidation status into monitor

**Objective:** Update the monitor to report actual consolidation status instead of defaults.

**Files:**
- Modify: `crates/perspective-core/src/monitor.rs` (update ConsolidationStatus)

**Step 1: Add method to update consolidation status**

In monitor.rs, add:

```rust
pub fn update_consolidation(&self, report: &crate::engine::ConsolidationReport) {
    let mut status = self.consolidation.lock().unwrap();
    status.last_run = Some(Utc::now());
    status.duplicates_found = report.duplicates_found;
    status.promotable_count = report.promotable_count;
    status.communities = report.communities;
}
```

**Step 2: Call from consolidation loop**

In the consolidation loop (Task 3.2), after getting the report:

```rust
self.monitor.update_consolidation(&report);
```

**Step 3: Verify build**

Run: `cargo check -p perspective-core`
Expected: compiles

**Step 4: Commit**

```bash
git add crates/perspective-core/src/monitor.rs crates/perspective-core/src/engine.rs
git commit -m "feat: wire consolidation status into monitor"
```

---

## Phase 4: Fix Memory Type Reconstruction (20 min)

recall() always creates `Memory::Episodic` regardless of what the memory actually was. Need to store and retrieve the memory type.

### Task 4.1: Store memory type in vector payload

**Objective:** When storing a memory, include `memory_type` in the vector payload so recall can reconstruct the correct type.

**Files:**
- Modify: `crates/perspective-core/src/engine.rs` (store method, ~line 150-200)

**Step 1: Add memory_type to payload**

In the `store` method, when building the payload for qdrant, add:

```rust
payload.insert("memory_type".to_string(), serde_json::json!(req.memory_type));
```

**Step 2: Verify build**

Run: `cargo check -p perspective-core`
Expected: compiles

**Step 3: Commit**

```bash
git add crates/perspective-core/src/engine.rs
git commit -m "feat: store memory_type in vector payload for recall reconstruction"
```

---

### Task 4.2: Reconstruct correct memory type in recall

**Objective:** Use the stored `memory_type` to create the correct Memory variant instead of always Episodic.

**Files:**
- Modify: `crates/perspective-core/src/engine.rs` (recall method, ~line 525-543)

**Step 1: Read memory_type from payload and create correct variant**

Replace the hardcoded `Memory::Episodic` block with:

```rust
let memory_type_str = payload
    .get("memory_type")
    .and_then(|v| v.as_str())
    .unwrap_or("episodic");

let memory = match memory_type_str {
    "semantic" => Memory::Semantic(SemanticMemory {
        base: base.clone(),
        confidence: 0.5,
        source_ids: vec![],
        access_count: 0,
        last_accessed: Utc::now(),
        stability: 10.0,
        first_seen: base.created_at,
        last_validated: None,
    }),
    "procedural" => Memory::Procedural(ProceduralMemory {
        base: base.clone(),
        code: None,
        preconditions: vec![],
        postconditions: vec![],
        success_rate: 1.0,
        access_count: 0,
        last_used: Utc::now(),
        stability: f32::INFINITY,
        version: 1,
    }),
    _ => Memory::Episodic(EpisodicMemory {
        base,
        timestamp: Utc::now(),
        context: None,
        importance: 0.5,
        access_count: 0,
        last_accessed: Utc::now(),
        stability: 1.0,
        source_session: None,
    }),
};

memories.push(memory);
```

**Step 2: Verify build**

Run: `cargo check -p perspective-core`
Expected: compiles

**Step 3: Commit**

```bash
git add crates/perspective-core/src/engine.rs
git commit -m "feat: reconstruct correct memory type in recall from stored payload"
```

---

## Phase 5: Clean Up Retrieval Duplication (15 min)

recall() duplicates logic from `retrieval/fusion.rs` and ignores `retrieval/entity_search.rs`. Clean this up.

### Task 5.1: Use fusion module instead of inline RRF

**Objective:** Replace inline RRF in recall() with the standalone `rrf_fuse()` function.

**Files:**
- Modify: `crates/perspective-core/src/engine.rs` (recall method, lines 414-424)

**Step 1: Import fusion module**

```rust
use crate::retrieval::fusion::rrf_fuse;
```

**Step 2: Replace inline RRF with rrf_fuse()**

Replace lines 414-424 with:

```rust
// Fuse vector and text results via RRF
let all_results: Vec<(Uuid, f32)> = vector_results
    .iter()
    .map(|r| (r.id, 1.0))  // placeholder score, rrf_fuse uses rank
    .chain(text_results.iter().map(|r| (r.id, 1.0)))
    .collect();
let fused = rrf_fuse(&all_results, self.config.retrieval.rrf_k);
let mut scores: std::collections::HashMap<Uuid, f32> = fused.into_iter().collect();
```

Note: Need to check the exact signature of `rrf_fuse()` and adapt. The current inline code works on ranked lists; the module function may have a different interface.

**Step 3: Verify build**

Run: `cargo check -p perspective-core`
Expected: compiles

**Step 4: Commit**

```bash
git add crates/perspective-core/src/engine.rs
git commit -m "refactor: use fusion module instead of inline RRF in recall()"
```

---

### Task 5.2: Add expose consolidation status in Python bindings

**Objective:** Expose `run_consolidation()` and `status_json()` (which now includes real consolidation data) to Python.

**Files:**
- Modify: `crates/perspective-python/src/lib.rs` (add method)

**Step 1: Add run_consolidation method**

```python
#[pyo3(signature = (tenant_id))]
fn run_consolidation(&self, tenant_id: &str) -> PyResult<String> {
    let report = self.runtime.block_on(async {
        self.inner.run_consolidation(tenant_id).await
    }).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{e}")))?;
    serde_json::to_string(&report).map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{e}")))
}
```

**Step 2: Verify build**

Run: `cargo check -p perspective-python`
Expected: compiles

**Step 3: Commit**

```bash
git add crates/perspective-python/src/lib.rs
git commit -m "feat: expose run_consolidation() in Python bindings"
```

---

## Phase 6: Activity Detail View in Dashboard (30 min)

The React dashboard expects `details_json` and `event_type` fields on ActivityEvent, but the Rust backend has `operation`, `content`, `success`. Need to bridge this gap and add click-to-expand details.

### Task 6.1: Add details_json to ActivityEvent and SQLite

**Objective:** Store rich details (query, result count, content preview) per activity event.

**Files:**
- Modify: `crates/perspective-core/src/monitor.rs` (ActivityEvent struct, SQLite schema, record_event)

**Step 1: Add details_json field to ActivityEvent**

```rust
pub struct ActivityEvent {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub operation: String,
    pub memory_type: Option<String>,
    pub content: Option<String>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details_json: Option<String>,
}
```

**Step 2: Add column to SQLite schema**

In `Monitor::new()`, add to the CREATE TABLE:
```sql
details_json TEXT
```

And add migration for existing DBs:
```sql
ALTER TABLE events ADD COLUMN details_json TEXT;
```

(Use try-catch pattern since ALTER TABLE fails if column already exists)

**Step 3: Update record_event to accept details**

```rust
pub fn record_event(
    &self,
    operation: &str,
    memory_type: Option<&str>,
    content: Option<&str>,
    success: bool,
    details_json: Option<&str>,  // NEW
) {
```

Update the INSERT to include `details_json`.

**Step 4: Update all call sites in engine.rs**

In `store()`: `self.monitor.record_event("store", Some(&req.memory_type.to_string()), Some(&req.content), true, None);`

In `recall()`: Build a details JSON with query and result count:
```rust
let details = serde_json::json!({
    "query": query,
    "result_count": memories.len(),
    "budget": budget,
}).to_string();
self.monitor.record_event("recall", None, Some(query), true, Some(&details));
```

In `reflect()`: Similar details with query.

**Step 5: Add get_event(id) method to Monitor**

```rust
pub fn get_event(&self, event_id: i64) -> Option<ActivityEvent> {
    // Query SQLite by ID
}
```

**Step 6: Add /api/activity/:id endpoint to server**

In main.rs, add a route that extracts the event ID from the path and returns full details.

**Step 7: Verify build**

Run: `cargo check`
Expected: compiles

**Step 8: Commit**

```bash
git add crates/perspective-core/src/monitor.rs crates/perspective-core/src/engine.rs crates/perspective-server/src/main.rs
git commit -m "feat: add details_json to activity events, store query/results for recall"
```

---

### Task 6.2: Update React dashboard for expandable activity details

**Objective:** Click an activity event to see full details (query, results, content).

**Files:**
- Modify: `dashboard/src/pages/Overview.tsx` (activity section)
- Modify: `dashboard/src/api.ts` (add getActivityEvent)

**Step 1: Add getActivityEvent to api.ts**

```typescript
getActivityEvent: (id: number) =>
  fetchJson<ActivityEvent>(`/api/activity/${id}`),
```

**Step 2: Make activity events clickable**

Replace the activity feed section with expandable items:

```tsx
{events.map((ev, i) => (
  <ActivityItem key={i} event={ev} />
))}
```

Add an `ActivityItem` component:
```tsx
function ActivityItem({ event }: { event: ActivityEvent }) {
  const [expanded, setExpanded] = useState(false);
  const [details, setDetails] = useState<any>(null);

  const handleClick = async () => {
    if (!expanded && event.details_json) {
      try {
        setDetails(JSON.parse(event.details_json));
      } catch {}
    }
    setExpanded(!expanded);
  };

  return (
    <div className="py-1.5 text-sm cursor-pointer hover:bg-zinc-800/50 rounded px-2 -mx-2" onClick={handleClick}>
      <div className="flex items-center gap-3">
        <span className="text-xs text-zinc-600 w-16 shrink-0">{formatTime(event.timestamp)}</span>
        <span className={`px-2 py-0.5 rounded text-xs font-medium ${eventColors[event.event_type] ?? 'bg-zinc-800 text-zinc-400'}`}>
          {event.event_type}
        </span>
        <span className="text-zinc-400 truncate flex-1">
          {details?.preview ?? event.memory_id ?? ''}
        </span>
      </div>
      {expanded && details && (
        <div className="ml-19 mt-2 p-3 bg-zinc-800/50 rounded-lg text-xs space-y-1">
          {details.query && <div><span className="text-zinc-500">Query:</span> <span className="text-zinc-300">{details.query}</span></div>}
          {details.result_count !== undefined && <div><span className="text-zinc-500">Results:</span> <span className="text-zinc-300">{details.result_count}</span></div>}
          {details.budget && <div><span className="text-zinc-500">Budget:</span> <span className="text-zinc-300">{details.budget}</span></div>}
          {details.content && <div><span className="text-zinc-500">Content:</span> <span className="text-zinc-300">{details.content}</span></div>}
          {details.memory_type && <div><span className="text-zinc-500">Type:</span> <span className="text-zinc-300">{details.memory_type}</span></div>}
          {details.success !== undefined && <div><span className="text-zinc-500">Success:</span> <span className={details.success ? 'text-green-400' : 'text-red-400'}>{details.success ? 'Yes' : 'No'}</span></div>}
        </div>
      )}
    </div>
  );
}
```

**Step 3: Add useState import**

```tsx
import { useState } from 'react';
```

**Step 4: Verify build**

Run: `cd dashboard && npm run build`
Expected: builds without errors

**Step 5: Commit**

```bash
git add dashboard/src/pages/Overview.tsx dashboard/src/api.ts
git commit -m "feat: expandable activity details in dashboard (query, results, content)"
```

---

### Task 6.3: Update embedded HTML dashboard for activity details

**Objective:** Same expandable details in the fallback HTML dashboard.

**Files:**
- Modify: `crates/perspective-server/src/dashboard.rs` (activity section)

**Step 1: Add click handler to activity rows**

In the JavaScript section, make activity rows clickable and show details_json content in a toggle div.

**Step 2: Verify build**

Run: `cargo check -p perspective-server`
Expected: compiles

**Step 3: Commit**

```bash
git add crates/perspective-server/src/dashboard.rs
git commit -m "feat: expandable activity details in embedded HTML dashboard"
```

---

## Summary

| Phase | What | Est. Time | Impact |
|-------|------|-----------|--------|
| 1 | Kill gRPC | 10 min | Removes dead deps, fixes misleading banner |
| 2 | Wire decay | 30 min | Recall filters by strength, memories reinforce on access, background loop |
| 3 | Wire consolidation | 30 min | Dedup/promotion/communities actually run on schedule |
| 4 | Fix memory types | 20 min | Recall returns correct type instead of always Episodic |
| 5 | Clean up retrieval | 15 min | Remove code duplication, expose consolidation to Python |
| 6 | Activity detail view | 30 min | Click activity item to see query, results, content |

**Total: ~135 min**

## After implementation

Update docs to match:
- ARCHITECTURE.md: remove gRPC references, mark decay/consolidation as "wired and running"
- README.md: remove "gRPC" from dual mode, add decay/consolidation as working features
- AGENTS.md: update accordingly

## Risks

1. **Decay reinforcement on recall is in-memory only** - vector store payload isn't updated. Acceptable for now; the next store() of similar content picks up new values. Full persistence would require re-indexing (expensive).

2. **Consolidation dedup/promotion are report-only for now** - the functions detect what to merge/promote but don't actually do it yet. The merge logic (keeping richer version, updating graph edges) is non-trivial and should be a separate task.

3. **New memories stored after Phase 4 will have memory_type in payload. Existing memories won't.** Recall will default those to "episodic" which is the current behavior. No migration needed.

4. **Background loops add tokio tasks** - minimal overhead, but worth monitoring in production.
