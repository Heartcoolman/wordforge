from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional


REPO_ROOT = Path(__file__).resolve().parents[2]
ADAPTER_BINARY = REPO_ROOT / "target" / "release" / "maimemo_mdm_adapter"


def ensure_adapter_binary() -> Path:
    # 总是构建（新鲜时 ~0.2s no-op）：mdm.rs 在本分支有改动，
    # 仅在缺失时构建会让陈旧二进制静默用旧模型打分
    subprocess.run(
        ["cargo", "build", "--release", "--bin", "maimemo_mdm_adapter"],
        cwd=REPO_ROOT,
        check=True,
    )
    return ADAPTER_BINARY


class AdapterServer:
    """Persistent JSONL server wrapping the Rust adapter binary.

    Usage::

        with AdapterServer() as server:
            results = server.score_batch(config, items)
    """

    def __init__(self) -> None:
        self._process: Optional[subprocess.Popen] = None

    def __enter__(self) -> "AdapterServer":
        binary = ensure_adapter_binary()
        self._process = subprocess.Popen(
            [str(binary)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=REPO_ROOT,
        )
        return self

    def __exit__(self, *args: object) -> None:
        if self._process is not None:
            try:
                self._process.stdin.close()
            except Exception:
                pass
            self._process.wait(timeout=5)
            self._process = None

    def score_batch(
        self,
        memory_config: Dict[str, Any],
        items: Iterable[Dict[str, Any]],
    ) -> List[Dict[str, Any]]:
        """Send one batch via JSONL, read one response line."""
        if self._process is None or self._process.poll() is not None:
            raise RuntimeError("AdapterServer is not running")
        request = {"config": memory_config, "items": list(items)}
        line = json.dumps(request, separators=(",", ":")) + "\n"
        self._process.stdin.write(line.encode("utf-8"))
        self._process.stdin.flush()
        response_line = self._process.stdout.readline()
        if not response_line:
            stderr = self._process.stderr.read().decode("utf-8", errors="replace")
            raise RuntimeError(f"Adapter server closed unexpectedly: {stderr}")
        payload = json.loads(response_line)
        return payload["items"]


def score_histories(
    memory_config: Dict[str, Any],
    items: Iterable[Dict[str, Any]],
    server: Optional[AdapterServer] = None,
) -> List[Dict[str, Any]]:
    """Score a batch of histories.

    If *server* is provided, uses the persistent connection.
    Otherwise falls back to a one-shot subprocess (legacy path).
    """
    if server is not None:
        return server.score_batch(memory_config, items)

    # Legacy one-shot fallback
    binary = ensure_adapter_binary()
    request = {"config": memory_config, "items": list(items)}
    process = subprocess.run(
        [str(binary), "--oneshot"],
        input=json.dumps(request).encode("utf-8"),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=REPO_ROOT,
        check=True,
    )
    payload = json.loads(process.stdout.decode("utf-8"))
    return payload["items"]
