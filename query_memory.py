#!/usr/bin/env python3
"""query_memory.py — RAG retrieval over the MultiTunnel agentic memory graph.

Any agent (session start, subagent, future session) can pull relevant project
memory with one command from the repo root:

    python3 query_memory.py "saturation aware traffic split"
    python3 query_memory.py "tray icon quick settings" --prompt
    python3 query_memory.py "verification gates" -k 8 --json

Retrieval = BM25 (lexical, title-boosted) + graph-neighborhood expansion
(typed edges, 0.5^hop score decay). Modes:
    default   ranked human-readable passages with sources + graph links
    --json    structured results + induced subgraph (for programmatic agents)
    --prompt  paste-ready RAG context pack (inject into any agent prompt)

The store is rebuilt automatically if missing. It lives at
harness/memory/store/memory_graph.json; rebuild it with
`python3 harness/memory/build_graph.py` (also run by update_memory.py).
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent
sys.path.insert(0, str(REPO / "harness" / "memory"))

import graph_store  # noqa: E402
from build_graph import DEFAULT_OUT  # noqa: E402


def ensure_store(store_path: Path) -> dict:
    if not store_path.exists():
        print(f"[i] store missing — building {store_path}", file=sys.stderr)
        subprocess.run(
            [sys.executable, str(REPO / "harness" / "memory" / "build_graph.py")],
            check=True)
    return graph_store.load_store(store_path)


def main() -> int:
    ap = argparse.ArgumentParser(
        description="RAG retrieval over the MultiTunnel memory graph")
    ap.add_argument("query", help="what to look for (natural language or keywords)")
    ap.add_argument("-k", type=int, default=5,
                    help="top hits (default 5; neighbors may add more)")
    ap.add_argument("--expand", type=int, default=1,
                    help="graph expansion hops (default 1; 0 = BM25 only)")
    ap.add_argument("--json", action="store_true", dest="as_json",
                    help="structured JSON output")
    ap.add_argument("--prompt", action="store_true",
                    help="paste-ready RAG context pack for a consuming agent")
    ap.add_argument("--store", type=Path, default=DEFAULT_OUT,
                    help="store path (default: %(default)s)")
    args = ap.parse_args()

    store = ensure_store(args.store)
    results = graph_store.retrieve(store, args.query, k=args.k,
                                   expand=args.expand)

    if args.as_json:
        ids = [r["node"] for r in results]
        print(json.dumps({
            "query": args.query,
            "k": args.k,
            "expand": args.expand,
            "as_of": store.get("as_of"),
            "results": results,
            "subgraph": graph_store.induced_subgraph(store, set(ids)),
        }, ensure_ascii=False, indent=2))
    elif args.prompt:
        print(graph_store.format_prompt(args.query, results, store))
    else:
        print(graph_store.format_human(args.query, results))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
