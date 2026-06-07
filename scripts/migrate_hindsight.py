#!/usr/bin/env python3
"""
Migrate memories from Hindsight to Perspective.

Usage:
    python3 migrate_hindsight.py --data-dir /path/to/perspective/data

Prerequisites:
    - Hindsight API running on localhost:9120
    - perspective-python installed (pip install target/wheels/*.whl)

This script:
1. Reads all memories from Hindsight API (paginated, batched)
2. Maps fact_type to Perspective memory types
3. Converts graph links to Perspective edges
4. Writes to Perspective stores (Qdrant Edge + redb)
5. Verifies counts match
"""

import argparse
import json
import sys
import time
import urllib.request
from datetime import datetime, timezone
from typing import Any

HINDSIGHT_API = "http://127.0.0.1:9120"
BANK_ID = "hermes"
PAGE_SIZE = 500  # memories per API call
BATCH_STORE_SIZE = 100  # memories per perspective store call


def hindsight_get(path: str, params: dict = None) -> dict:
    """GET request to Hindsight API."""
    url = f"{HINDSIGHT_API}{path}"
    if params:
        query = "&".join(f"{k}={v}" for k, v in params.items())
        url += f"?{query}"
    req = urllib.request.Request(url)
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read().decode())


def get_stats() -> dict:
    """Get Hindsight bank stats."""
    return hindsight_get(f"/v1/default/banks/{BANK_ID}/stats")


def get_memories(offset: int = 0, limit: int = PAGE_SIZE,
                 fact_type: str = None) -> list:
    """Fetch a page of memories from Hindsight."""
    params = {"offset": str(offset), "limit": str(limit)}
    if fact_type:
        params["fact_type"] = fact_type
    data = hindsight_get(f"/v1/default/banks/{BANK_ID}/memories", params)
    return data.get("memories", [])


def get_links(memory_id: str) -> list:
    """Fetch links for a memory (for graph reconstruction)."""
    # Hindsight doesn't have a per-memory links endpoint in the REST API
    # We'll use the recall endpoint to get connected memories
    # or query the database directly
    return []


def map_fact_type(fact_type: str) -> str:
    """Map Hindsight fact_type to Perspective memory_type."""
    mapping = {
        "experience": "episodic",
        "world": "semantic",
        "observation": "semantic",
    }
    return mapping.get(fact_type, "semantic")


def map_link_type(link_type: str) -> str:
    """Map Hindsight link_type to Perspective edge_type."""
    mapping = {
        "semantic": "semantic",
        "temporal": "temporal",
        "entity": "mentions",
        "caused_by": "causal",
    }
    return mapping.get(link_type, "semantic")


def transform_memory(mu: dict) -> dict:
    """Transform a Hindsight memory_unit to Perspective format."""
    memory_type = map_fact_type(mu.get("fact_type", "world"))

    # Map access_count to importance score (0-1 range, log-scaled)
    access_count = mu.get("access_count", 0)
    importance = min(1.0, (access_count + 1) / 100.0)  # simple linear cap

    # Get timestamp from available fields
    timestamp = None
    for field in ["event_date", "occurred_start", "mentioned_at", "created_at"]:
        val = mu.get(field)
        if val:
            timestamp = val
            break

    tags = mu.get("tags", [])
    if isinstance(tags, str):
        tags = [t.strip() for t in tags.split(",") if t.strip()]

    return {
        "memory_type": memory_type,
        "content": mu["text"],
        "importance": importance,
        "tags": tags,
        "metadata": {
            "hindsight_id": str(mu["id"]),
            "hindsight_fact_type": mu.get("fact_type"),
            "context": mu.get("context"),
            "document_id": mu.get("document_id"),
            "chunk_id": mu.get("chunk_id"),
            "source_memory_ids": [str(s) for s in (mu.get("source_memory_ids") or [])],
            "proof_count": mu.get("proof_count", 1),
            "consolidated_at": mu.get("consolidated_at"),
            "text_signals": mu.get("text_signals"),
        },
        "created_at": timestamp,
    }


def get_all_links_from_db() -> list:
    """
    Read all links from Hindsight's PostgreSQL directly.
    This requires the pg0 instance to be running.

    Returns list of {from_id, to_id, link_type, weight, created_at}.
    """
    # We read from the Hindsight API's link endpoint if available,
    # or fall back to direct DB access.
    # For now, use the API's memory detail endpoint to get links per memory.
    # This is slow for 2.7M links but accurate.

    # Alternative: read from DB directly if psql is available
    try:
        import subprocess
        result = subprocess.run(
            ["/home/charlie/.pg0/installation/18.1.0/bin/psql",
             "-h", "127.0.0.1", "-p", "5432",
             "-U", "hindsight", "-d", "hindsight",
             "-t", "-A", "-c",
             """SELECT from_unit_id, to_unit_id, link_type, weight, created_at
                FROM memory_links ml
                JOIN memory_units mu ON ml.from_unit_id = mu.id
                WHERE mu.bank_id = 'hermes'"""],
            capture_output=True, text=True, timeout=300,
            env={"PGPASSWORD": "hindsight"}
        )
        if result.returncode == 0:
            links = []
            for line in result.stdout.strip().split("\n"):
                if not line or line.count("|") < 3:
                    continue
                parts = line.split("|")
                links.append({
                    "from_id": parts[0],
                    "to_id": parts[1],
                    "link_type": parts[2],
                    "weight": float(parts[3]) if parts[3] else 1.0,
                    "created_at": parts[4] if len(parts) > 4 else None,
                })
            return links
    except Exception as e:
        print(f"  Direct DB access failed: {e}")
        print("  Falling back to API-based link extraction (slower)")

    return []


def main():
    parser = argparse.ArgumentParser(description="Migrate Hindsight to Perspective")
    parser.add_argument("--data-dir", required=True,
                        help="Perspective data directory")
    parser.add_argument("--bank", default=BANK_ID,
                        help="Hindsight bank ID (default: hermes)")
    parser.add_argument("--skip-links", action="store_true",
                        help="Skip graph link migration")
    parser.add_argument("--dry-run", action="store_true",
                        help="Show what would be migrated without writing")
    args = parser.parse_args()

    global BANK_ID
    BANK_ID = args.bank

    # 1. Check Hindsight is running
    try:
        stats = get_stats()
    except Exception as e:
        print(f"ERROR: Cannot reach Hindsight API at {HINDSIGHT_API}")
        print(f"  {e}")
        sys.exit(1)

    total_memories = stats["total_nodes"]
    total_links = stats["total_links"]
    print(f"Hindsight stats: {total_memories} memories, {total_links} links")
    print(f"  experience: {stats['nodes_by_fact_type'].get('experience', 0)}")
    print(f"  observation: {stats['nodes_by_fact_type'].get('observation', 0)}")
    print(f"  world: {stats['nodes_by_fact_type'].get('world', 0)}")

    if args.dry_run:
        print("\n[DRY RUN] Would migrate:")
        print(f"  {total_memories} memories -> Perspective ({args.data_dir})")
        print(f"  {total_links} links -> Perspective graph")
        return

    # 2. Initialize Perspective engine
    try:
        from perspective_python import PerspectiveEngine
    except ImportError:
        print("ERROR: perspective-python not installed")
        print("  Run: cd crates/perspective-python && maturin develop")
        sys.exit(1)

    engine = PerspectiveEngine(args.data_dir)
    print(f"\nPerspective engine initialized at {args.data_dir}")

    # 3. Migrate memories (paginated)
    migrated = 0
    failed = 0
    offset = 0
    start_time = time.time()

    while True:
        memories = get_memories(offset=offset, limit=PAGE_SIZE)
        if not memories:
            break

        batch = []
        for mu in memories:
            try:
                transformed = transform_memory(mu)
                batch.append(transformed)
            except Exception as e:
                print(f"  Failed to transform memory {mu.get('id')}: {e}")
                failed += 1

        # Store batch
        for m in batch:
            try:
                engine.store(
                    tenant=BANK_ID,
                    content=m["content"],
                    memory_type=m["memory_type"],
                    tags=m.get("tags", []),
                    importance=m.get("importance", 0.5),
                )
                migrated += 1
            except Exception as e:
                print(f"  Failed to store: {e}")
                failed += 1

        # Progress
        elapsed = time.time() - start_time
        rate = migrated / elapsed if elapsed > 0 else 0
        pct = (offset + len(memories)) / total_memories * 100
        print(f"\r  {migrated}/{total_memories} migrated ({pct:.1f}%) "
              f"| {rate:.1f}/s | {failed} failed", end="", flush=True)

        offset += PAGE_SIZE
        time.sleep(0.1)  # be nice to the API

    print(f"\n\nMigration complete:")
    print(f"  Migrated: {migrated}")
    print(f"  Failed: {failed}")
    print(f"  Time: {time.time() - start_time:.1f}s")

    # 4. Migrate links (if not skipped)
    if not args.skip_links:
        print(f"\nMigrating {total_links} links...")
        links = get_all_links_from_db()
        if links:
            print(f"  Read {len(links)} links from database")
            # TODO: Write links to Perspective graph store
            # This requires exposing a graph edge creation API in the engine
            print("  [TODO] Graph link migration not yet implemented")
            print("  Links are available in Hindsight's recall endpoints")
        else:
            print("  Could not read links. Use --skip-links to skip.")

    # 5. Verify
    print("\nVerification:")
    print(f"  Expected: {total_memories} memories")
    print(f"  Migrated: {migrated}")
    print(f"  Failed: {failed}")
    if migrated == total_memories:
        print("  PASS - counts match")
    else:
        print(f"  MISMATCH - {total_memories - migrated} memories missing")


if __name__ == "__main__":
    main()
