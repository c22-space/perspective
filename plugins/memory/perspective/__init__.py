"""Perspective memory plugin — MemoryProvider interface.

Direct in-process memory provider using perspective-python (PyO3 bindings).
No HTTP server needed. The Rust engine runs inside the Python process.

Config via $HERMES_HOME/perspective/config.json:
  data_dir     — Path to store perspective data (default: ~/.hermes/perspective/data)
  tenant_id    — Tenant identifier (default: hermes)
  budget       — Recall budget / max results (default: 10)
"""

from __future__ import annotations

import json
import logging
import os
from pathlib import Path
from typing import Any, Dict, List, Optional

from agent.memory_provider import MemoryProvider

logger = logging.getLogger(__name__)

_DEFAULT_DATA_DIR = os.path.join(os.path.expanduser("~"), ".hermes", "perspective", "data")
_DEFAULT_TENANT_ID = "hermes"
_DEFAULT_BUDGET = 10


def _load_config(hermes_home: str) -> dict:
    """Load config from profile-scoped path or fall back to defaults."""
    config_path = Path(hermes_home) / "perspective" / "config.json"
    config: dict[str, Any] = {
        "data_dir": os.environ.get(
            "PERSPECTIVE_DATA_DIR", _DEFAULT_DATA_DIR
        ),
        "tenant_id": os.environ.get(
            "PERSPECTIVE_TENANT_ID", _DEFAULT_TENANT_ID
        ),
        "budget": _DEFAULT_BUDGET,
    }
    if config_path.exists():
        try:
            raw = json.loads(config_path.read_text(encoding="utf-8"))
            if isinstance(raw, dict):
                config.update({k: v for k, v in raw.items() if v is not None})
        except Exception:
            logger.debug("Failed to parse %s", config_path, exc_info=True)
    return config


class PerspectiveMemoryProvider(MemoryProvider):
    """Perspective memory engine — in-process Rust-backed memory provider."""

    def __init__(self):
        self._engine = None
        self._tenant_id: str = _DEFAULT_TENANT_ID
        self._budget: int = _DEFAULT_BUDGET
        self._session_id: str = ""
        self._active: bool = False

    @property
    def name(self) -> str:
        return "perspective"

    def is_available(self) -> bool:
        """Check if perspective-python is importable and engine can start."""
        try:
            from perspective_python import PerspectiveEngine  # noqa: F401
            return True
        except ImportError:
            return False
        except Exception:
            return False

    def initialize(self, session_id: str, **kwargs) -> None:
        """Initialize session, load config, create engine."""
        try:
            from perspective_python import PerspectiveEngine
        except ImportError:
            logger.warning(
                "perspective-python not installed. "
                "Install with: maturin develop (from perspective repo)"
            )
            return

        hermes_home = kwargs.get("hermes_home", str(Path.home() / ".hermes"))
        config = _load_config(hermes_home)

        data_dir = config.get("data_dir", _DEFAULT_DATA_DIR)
        self._tenant_id = config.get("tenant_id", _DEFAULT_TENANT_ID)
        self._budget = config.get("budget", _DEFAULT_BUDGET)
        self._session_id = session_id

        # Ensure data directory exists
        Path(data_dir).mkdir(parents=True, exist_ok=True)

        try:
            self._engine = PerspectiveEngine(data_dir)
            self._active = True
            logger.debug(
                "Perspective initialized: data_dir=%s, tenant=%s, budget=%s",
                data_dir, self._tenant_id, self._budget,
            )
        except Exception as e:
            logger.warning("Failed to create Perspective engine: %s", e)
            self._engine = None

    def system_prompt_block(self) -> str:
        """Return static system prompt text indicating Perspective is active."""
        if self._active:
            return "Perspective memory engine active (in-process Rust)"
        return ""

    def prefetch(self, query: str, *, session_id: str = "") -> str:
        """Recall relevant context from Perspective for the upcoming turn.

        Returns formatted context string.
        Returns empty string on failure (graceful degradation).
        """
        if not self._active or self._engine is None:
            return ""

        sid = session_id or self._session_id

        try:
            results = self._engine.recall(
                self._tenant_id, query, self._budget
            )
            if not results:
                return ""

            lines = []
            for r in results:
                if hasattr(r, "content") and r.content:
                    score_str = f"{r.score:.3f}" if hasattr(r, "score") else "?"
                    lines.append(
                        f"- [{r.memory_type}, score={score_str}] {r.content}"
                    )

            if not lines:
                return ""

            return (
                "The following is background context from long-term memory. "
                "Use it silently when relevant.\n\n"
                + "\n".join(lines)
            )
        except Exception as exc:
            logger.debug("Perspective recall failed: %s", exc)
            return ""

    def sync_turn(
        self,
        user_content: str,
        assistant_content: str,
        *,
        session_id: str = "",
        messages: Optional[List[Dict[str, Any]]] = None,
    ) -> None:
        """Persist a completed turn to the Perspective engine.

        Stores both user and assistant turns as episodic memories.
        Failures are logged but do not raise.
        """
        if not self._active or self._engine is None:
            return

        sid = session_id or self._session_id

        for role, content in [("user", user_content), ("assistant", assistant_content)]:
            if not content:
                continue
            try:
                self._engine.store(
                    tenant_id=self._tenant_id,
                    content=content,
                    memory_type="episodic",
                    tags=[f"role:{role}"],
                    session_id=sid,
                )
            except Exception as exc:
                logger.debug("Perspective store failed for role=%s: %s", role, exc)

    def get_tool_schemas(self) -> List[Dict[str, Any]]:
        """Return empty list — this is a context-only provider with no tools."""
        return []

    def on_memory_write(
        self,
        action: str,
        target: str,
        content: str,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> None:
        """Mirror built-in memory writes to Perspective."""
        if not self._active or self._engine is None:
            return
        try:
            if action == "remove":
                return  # No delete support from memory tool yet
            tags = [f"memory:{target}", f"action:{action}"]
            if metadata and metadata.get("write_origin"):
                tags.append(f"origin:{metadata['write_origin']}")
            self._engine.store(
                tenant_id=self._tenant_id,
                content=content,
                memory_type="semantic",
                tags=tags,
            )
        except Exception as exc:
            logger.debug("Perspective on_memory_write failed: %s", exc)

    def on_delegation(self, task: str, result: str, *, child_session_id: str = "", **kwargs) -> None:
        """Store delegation task/result as episodic memory."""
        if not self._active or self._engine is None:
            return
        try:
            self._engine.store(
                tenant_id=self._tenant_id,
                content=f"Delegated task: {task}\nResult: {result[:500]}",
                memory_type="episodic",
                tags=["role:delegation", f"child:{child_session_id}"],
            )
        except Exception as exc:
            logger.debug("Perspective on_delegation failed: %s", exc)

    def on_pre_compress(self, messages: List[Dict[str, Any]]) -> str:
        """Extract insights from messages about to be compressed."""
        if not self._active or self._engine is None:
            return ""
        # Store the compressed messages as episodic memory
        try:
            summary_parts = []
            for msg in messages[-5:]:  # Last 5 messages
                role = msg.get("role", "unknown")
                content = msg.get("content", "")
                if isinstance(content, str) and content:
                    summary_parts.append(f"{role}: {content[:200]}")
            if summary_parts:
                self._engine.store(
                    tenant_id=self._tenant_id,
                    content="\n".join(summary_parts),
                    memory_type="episodic",
                    tags=["role:compression"],
                )
        except Exception:
            pass
        return ""

    def shutdown(self) -> None:
        """Clean shutdown."""
        self._active = False
        self._engine = None
        logger.debug("Perspective shutdown complete")

    def on_session_switch(
        self,
        new_session_id: str,
        *,
        parent_session_id: str = "",
        reset: bool = False,
        rewound: bool = False,
        **kwargs,
    ) -> None:
        """Update session_id when the agent switches sessions."""
        self._session_id = new_session_id

    def on_session_end(self, messages: List[Dict[str, Any]]) -> None:
        """Flush remaining conversation context at session end."""
        # The engine persists everything inline via sync_turn,
        # so no additional flush needed here.
        pass

    def save_config(self, values: dict, hermes_home: str) -> None:
        """Write config to $HERMES_HOME/perspective/config.json."""
        config_dir = Path(hermes_home) / "perspective"
        config_dir.mkdir(parents=True, exist_ok=True)
        config_path = config_dir / "config.json"
        existing: dict = {}
        if config_path.exists():
            try:
                existing = json.loads(config_path.read_text(encoding="utf-8"))
            except Exception:
                pass
        existing.update(values)
        config_path.write_text(
            json.dumps(existing, indent=2),
            encoding="utf-8",
        )

    def get_config_schema(self) -> List[Dict[str, Any]]:
        """Return config fields for `hermes memory setup`."""
        return [
            {
                "key": "data_dir",
                "description": "Path to store perspective data",
                "default": _DEFAULT_DATA_DIR,
            },
            {
                "key": "tenant_id",
                "description": "Tenant identifier",
                "default": _DEFAULT_TENANT_ID,
            },
            {
                "key": "budget",
                "description": "Recall budget (max results)",
                "default": _DEFAULT_BUDGET,
            },
        ]
