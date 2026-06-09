use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

/// A recorded operation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Consolidation status snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConsolidationStatus {
    pub running: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub items_processed: u64,
    pub merges: u64,
    pub promotions: u64,
}

/// Decay system status.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecayStatus {
    pub gc_candidates: u64,
    pub last_gc_run: Option<DateTime<Utc>>,
    pub items_collected: u64,
    pub avg_stability_episodic: Option<f32>,
    pub avg_stability_semantic: Option<f32>,
}

/// Extraction queue item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionItem {
    pub enqueued_at: DateTime<Utc>,
    pub source: String,
    pub preview: String,
    pub status: String,
}

/// Consolidation history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationRun {
    pub time: DateTime<Utc>,
    pub duration_ms: u64,
    pub memories_processed: u64,
    pub merges: u64,
    pub promotions: u64,
    pub gc_collected: u64,
}

/// Memory type breakdown.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryTypeCounts {
    pub episodic: u64,
    pub semantic: u64,
    pub procedural: u64,
}

/// Graph stats snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphStats {
    pub total_nodes: u64,
    pub total_edges: u64,
    pub communities: u64,
    pub avg_connectivity: f32,
    pub node_types: GraphNodeTypeCounts,
    pub edge_types: std::collections::HashMap<String, u64>,
    pub recent_edges: Vec<RecentEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphNodeTypeCounts {
    pub memory_ref: u64,
    pub entity: u64,
    pub concept: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentEdge {
    pub created_at: Option<DateTime<Utc>>,
    pub edge_type: String,
    pub from_id: String,
    pub to_id: String,
    pub weight: f32,
}

/// Full status snapshot for the dashboard API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub health: String,
    pub uptime_secs: u64,
    pub total_memories: u64,
    pub memory_types: MemoryTypeCounts,
    pub gc_candidates: u64,
    pub extraction_queue: usize,
    pub graph: GraphStats,
}

/// Processes page data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessesResponse {
    pub consolidation: ConsolidationStatus,
    pub decay: DecayStatus,
    pub extraction_queue: Vec<ExtractionItem>,
    pub consolidation_history: Vec<ConsolidationRun>,
}

/// Activity feed response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityResponse {
    pub events: Vec<ActivityEvent>,
}

/// Graph page response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphResponse {
    pub graph: GraphStats,
}

/// Memory list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoriesResponse {
    pub memories: Vec<MemorySummary>,
    pub total: u64,
}

/// Summary of a memory for the dashboard (no embedding data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySummary {
    pub id: String,
    pub memory_type: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub importance: Option<f32>,
    pub stability: Option<f32>,
    pub access_count: u32,
    pub last_accessed: DateTime<Utc>,
    pub source_session: Option<String>,
}

/// Engine config response for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub storage: std::collections::HashMap<String, String>,
    pub embedding: std::collections::HashMap<String, String>,
    pub decay: std::collections::HashMap<String, String>,
    pub retrieval: std::collections::HashMap<String, String>,
    pub consolidation: std::collections::HashMap<String, String>,
    pub extraction: std::collections::HashMap<String, String>,
}

/// Monitoring engine that tracks activity, status, and background process state.
/// Events and consolidation history are persisted to SQLite.
pub struct Monitor {
    /// SQLite connection for persistent storage.
    db: Mutex<Connection>,
    /// In-memory ring buffer for fast recent access (also in SQLite).
    events: Mutex<VecDeque<ActivityEvent>>,
    /// Max events to keep in memory ring buffer.
    max_events: usize,
    /// Engine start time.
    started_at: DateTime<Utc>,
    /// Consolidation status (ephemeral, not persisted).
    consolidation: Mutex<ConsolidationStatus>,
    /// Decay status (ephemeral, not persisted).
    decay: Mutex<DecayStatus>,
    /// Extraction queue (ephemeral, not persisted).
    extraction_queue: Mutex<Vec<ExtractionItem>>,
}

impl Monitor {
    pub fn new(data_dir: &Path) -> Self {
        let db_path = data_dir.join("activity.db");
        std::fs::create_dir_all(data_dir).ok();
        let conn = Connection::open(&db_path).expect("Failed to open activity SQLite database");

        // Create tables
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                operation TEXT NOT NULL,
                memory_type TEXT,
                content TEXT,
                success INTEGER NOT NULL DEFAULT 1,
                details_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_events_operation ON events(operation);

            CREATE TABLE IF NOT EXISTS consolidation_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                time TEXT NOT NULL,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                memories_processed INTEGER NOT NULL DEFAULT 0,
                merges INTEGER NOT NULL DEFAULT 0,
                promotions INTEGER NOT NULL DEFAULT 0,
                gc_collected INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_consolidation_runs_time ON consolidation_runs(time DESC);
            ",
        )
        .expect("Failed to create activity tables");

        // Migration: add details_json column if it doesn't exist
        {
            let _ = conn.execute("ALTER TABLE events ADD COLUMN details_json TEXT", []);
        }

        let mut ring_buffer = VecDeque::with_capacity(1000);

        // Warm the in-memory ring buffer from the last 1000 events
        {
            let stmt = conn.prepare(
                "SELECT id, timestamp, operation, memory_type, content, success, details_json
                 FROM events ORDER BY id DESC LIMIT 1000",
            );
            if let Ok(mut stmt) = stmt {
                let rows = stmt
                    .query_map([], |row| {
                        Ok(ActivityEvent {
                            id: row.get(0)?,
                            timestamp: row.get::<_, String>(1)?.parse().unwrap_or_default(),
                            operation: row.get(2)?,
                            memory_type: row.get(3)?,
                            content: row.get(4)?,
                            success: row.get::<_, i64>(5)? != 0,
                            details_json: row.get(6)?,
                        })
                    })
                    .expect("Failed to query events");
                for row in rows.flatten() {
                    ring_buffer.push_back(row);
                }
            }
        }

        Self {
            db: Mutex::new(conn),
            events: Mutex::new(ring_buffer),
            max_events: 1000,
            started_at: Utc::now(),
            consolidation: Mutex::new(ConsolidationStatus::default()),
            decay: Mutex::new(DecayStatus::default()),
            extraction_queue: Mutex::new(Vec::new()),
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        (Utc::now() - self.started_at).num_seconds() as u64
    }

    /// Record an activity event to SQLite and in-memory ring buffer.
    pub fn record_event(
        &self,
        operation: &str,
        memory_type: Option<&str>,
        content: Option<&str>,
        success: bool,
        details_json: Option<&str>,
    ) {
        let now = Utc::now();
        let ts = now.to_rfc3339();
        let content_truncated = content.map(|s| s.chars().take(200).collect::<String>());

        // Write to SQLite
        if let Ok(db) = self.db.lock() {
            let _ = db.execute(
                "INSERT INTO events (timestamp, operation, memory_type, content, success, details_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    ts,
                    operation,
                    memory_type,
                    content_truncated.as_deref(),
                    success as i64,
                    details_json,
                ],
            );
        }

        // Write to in-memory ring buffer
        let event = ActivityEvent {
            id: 0, // SQLite auto-generates the real ID
            timestamp: now,
            operation: operation.to_string(),
            memory_type: memory_type.map(|s| s.to_string()),
            content: content_truncated,
            success,
            details_json: details_json.map(|s| s.to_string()),
        };
        if let Ok(mut events) = self.events.lock() {
            events.push_back(event);
            while events.len() > self.max_events {
                events.pop_front();
            }
        }
    }

    /// Get recent activity events from SQLite.
    pub fn get_events(&self, limit: usize) -> Vec<ActivityEvent> {
        if let Ok(db) = self.db.lock() {
            let mut stmt = match db.prepare(
                "SELECT id, timestamp, operation, memory_type, content, success, details_json
                 FROM events ORDER BY id DESC LIMIT ?1",
            ) {
                Ok(s) => s,
                Err(_) => return vec![],
            };
            let rows = stmt.query_map(params![limit as i64], |row| {
                Ok(ActivityEvent {
                    id: row.get(0)?,
                    timestamp: row.get::<_, String>(1)?.parse().unwrap_or_default(),
                    operation: row.get(2)?,
                    memory_type: row.get(3)?,
                    content: row.get(4)?,
                    success: row.get::<_, i64>(5)? != 0,
                    details_json: row.get(6)?,
                })
            });
            if let Ok(rows) = rows {
                return rows.flatten().collect();
            }
        }
        // Fallback to in-memory ring buffer
        self.events
            .lock()
            .map(|events| events.iter().rev().take(limit).cloned().collect())
            .unwrap_or_default()
    }

    /// Get a single activity event by ID.
    pub fn get_event(&self, event_id: i64) -> Option<ActivityEvent> {
        if let Ok(db) = self.db.lock() {
            let mut stmt = match db.prepare(
                "SELECT id, timestamp, operation, memory_type, content, success, details_json
                 FROM events WHERE id = ?1",
            ) {
                Ok(s) => s,
                Err(_) => return None,
            };
            let row = stmt
                .query_row(params![event_id], |row| {
                    Ok(ActivityEvent {
                        id: row.get(0)?,
                        timestamp: row.get::<_, String>(1)?.parse().unwrap_or_default(),
                        operation: row.get(2)?,
                        memory_type: row.get(3)?,
                        content: row.get(4)?,
                        success: row.get::<_, i64>(5)? != 0,
                        details_json: row.get(6)?,
                    })
                });
            return row.ok();
        }
        None
    }

    /// Count total events.
    pub fn event_count(&self) -> u64 {
        if let Ok(db) = self.db.lock() {
            if let Ok(count) = db.query_row("SELECT COUNT(*) FROM events", [], |row| {
                row.get::<_, i64>(0)
            }) {
                return count as u64;
            }
        }
        0
    }

    /// Get consolidation status.
    pub fn consolidation_status(&self) -> ConsolidationStatus {
        self.consolidation
            .lock()
            .ok()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Update consolidation status.
    pub fn update_consolidation(&self, status: ConsolidationStatus) {
        if let Ok(mut s) = self.consolidation.lock() {
            *s = status;
        }
    }

    /// Record a consolidation run to SQLite.
    pub fn record_consolidation_run(&self, run: ConsolidationRun) {
        if let Ok(db) = self.db.lock() {
            let _ = db.execute(
                "INSERT INTO consolidation_runs
                 (time, duration_ms, memories_processed, merges, promotions, gc_collected)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    run.time.to_rfc3339(),
                    run.duration_ms as i64,
                    run.memories_processed as i64,
                    run.merges as i64,
                    run.promotions as i64,
                    run.gc_collected as i64,
                ],
            );
        }
    }

    /// Get consolidation history from SQLite.
    pub fn consolidation_history(&self) -> Vec<ConsolidationRun> {
        if let Ok(db) = self.db.lock() {
            let mut stmt = match db.prepare(
                "SELECT time, duration_ms, memories_processed, merges, promotions, gc_collected
                 FROM consolidation_runs ORDER BY id DESC LIMIT 100",
            ) {
                Ok(s) => s,
                Err(_) => return vec![],
            };
            let rows = stmt.query_map([], |row| {
                Ok(ConsolidationRun {
                    time: row.get::<_, String>(0)?.parse().unwrap_or_default(),
                    duration_ms: row.get::<_, i64>(1)? as u64,
                    memories_processed: row.get::<_, i64>(2)? as u64,
                    merges: row.get::<_, i64>(3)? as u64,
                    promotions: row.get::<_, i64>(4)? as u64,
                    gc_collected: row.get::<_, i64>(5)? as u64,
                })
            });
            if let Ok(rows) = rows {
                return rows.flatten().collect();
            }
        }
        vec![]
    }

    /// Get decay status.
    pub fn decay_status(&self) -> DecayStatus {
        self.decay
            .lock()
            .ok()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Update decay status.
    pub fn update_decay(&self, status: DecayStatus) {
        if let Ok(mut s) = self.decay.lock() {
            *s = status;
        }
    }

    /// Get extraction queue.
    pub fn extraction_queue(&self) -> Vec<ExtractionItem> {
        self.extraction_queue
            .lock()
            .ok()
            .map(|q| q.clone())
            .unwrap_or_default()
    }

    /// Update extraction queue.
    pub fn update_extraction_queue(&self, queue: Vec<ExtractionItem>) {
        if let Ok(mut q) = self.extraction_queue.lock() {
            *q = queue;
        }
    }
}
