#!/usr/bin/env python3
"""write_memory.py — the agent WRITE path into the MultiTunnel memory graph.

Agents (main session, subagents, future sessions) append durable knowledge
without touching the builder. Writes land in harness/memory/curated/
(merged last at build time — curated wins on node-id collision) and the
store is rebuilt immediately. Validation is strict (whitelisted types/rels,
endpoints must exist); a failed rebuild rolls the curated change back.

Usage (from repo root):

    python3 harness/memory/write_memory.py \\
        --node id=task:next-thing type=task title="Next thing" \\
        passage="What and why." tag=next source=harness/memory/LEDGER.md

    python3 harness/memory/write_memory.py \\
        --edge src=task:next-thing dst=project:multitunnel rel=part_of \\
        evidence="roadmap item"

    python3 harness/memory/write_memory.py --list          # curated ids
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import graph_store  # noqa: E402
from build_graph import DEFAULT_OUT, MEM, build  # noqa: E402

CURATED = MEM / "curated"


def parse_kv(items: list[str]) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for it in items:
        if "=" not in it:
            raise SystemExit(f"[!] expected KEY=VALUE, got: {it!r}")
        k, v = it.split("=", 1)
        out.setdefault(k, []).append(v)
    return out


def read_json(path: Path, default: dict) -> dict:
    if path.exists():
        return json.loads(path.read_text(encoding="utf-8"))
    return default


def write_json(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n",
                    encoding="utf-8")


def do_node(kv: dict[str, list[str]]) -> None:
    need = ("id", "type", "title", "passage")
    for k in need:
        if k not in kv:
            raise SystemExit(f"[!] --node requires {'/'.join(need)}=… (missing {k})")
    if kv["type"][0] not in graph_store.NODE_TYPES:
        raise SystemExit(f"[!] unknown node type {kv['type'][0]!r}; "
                         f"whitelist: {', '.join(graph_store.NODE_TYPES)}")
    node = {
        "id": kv["id"][0],
        "type": kv["type"][0],
        "title": kv["title"][0],
        "tags": kv.get("tag", []),
        "sources": [{"file": f, "anchor": ""}
                    for f in kv.get("source", ["harness/memory/curated/"])],
        "passages": [{"text": t, "kind": "body"} for t in kv["passage"]],
    }
    nodes = read_json(CURATED / "nodes.json", {"nodes": []})
    nodes["nodes"] = [n for n in nodes["nodes"] if n["id"] != node["id"]]
    nodes["nodes"].append(node)
    nodes["nodes"].sort(key=lambda n: n["id"])
    write_json(CURATED / "nodes.json", nodes)
    print(f"[✓] node upserted: {node['id']} ({len(nodes['nodes'])} curated)")


def do_edge(kv: dict[str, list[str]]) -> None:
    need = ("src", "dst", "rel")
    for k in need:
        if k not in kv:
            raise SystemExit(f"[!] --edge requires {'/'.join(need)}=… (missing {k})")
    if kv["rel"][0] not in graph_store.EDGE_RELS:
        raise SystemExit(f"[!] unknown rel {kv['rel'][0]!r}; "
                         f"whitelist: {', '.join(graph_store.EDGE_RELS)}")
    edge = {
        "src": kv["src"][0],
        "dst": kv["dst"][0],
        "rel": kv["rel"][0],
        "evidence": kv.get("evidence", ["agent-curated"])[0],
    }
    edges = read_json(CURATED / "edges.json", {"edges": []})
    edges["edges"] = [e for e in edges["edges"]
                      if (e["src"], e["dst"], e["rel"])
                      != (edge["src"], edge["dst"], edge["rel"])]
    edges["edges"].append(edge)
    edges["edges"].sort(key=lambda e: (e["src"], e["dst"], e["rel"]))
    write_json(CURATED / "edges.json", edges)
    print(f"[✓] edge upserted: {edge['src']} --{edge['rel']}--> {edge['dst']}")


def rebuild_with_rollback(store_out: Path) -> None:
    """Rebuild; on any failure restore the curated files from a snapshot."""
    with tempfile.TemporaryDirectory() as td:
        snap = Path(td)
        shutil.copytree(CURATED, snap / "curated")
        try:
            build(store_out, CURATED)
        except Exception as exc:  # rollback, then surface loudly
            shutil.rmtree(CURATED)
            shutil.copytree(snap / "curated", CURATED)
            try:
                build(store_out, CURATED)
            except Exception:
                pass
            raise SystemExit(f"[!] rebuild failed, curated change ROLLED BACK: {exc}")
    print(f"[✓] store rebuilt: {store_out}")


def main() -> int:
    global CURATED
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--node", nargs="+", metavar="KEY=VALUE",
                    help="node fields: id type title passage [tag source]")
    ap.add_argument("--edge", nargs="+", metavar="KEY=VALUE",
                    help="edge fields: src dst rel [evidence]")
    ap.add_argument("--list", action="store_true",
                    help="list curated node ids + edges")
    ap.add_argument("--no-rebuild", action="store_true",
                    help="skip the store rebuild (rarely wanted)")
    ap.add_argument("--curated", type=Path, default=CURATED,
                    help=argparse.SUPPRESS)  # test hook: curated dir override
    ap.add_argument("--store-out", type=Path, default=DEFAULT_OUT,
                    help=argparse.SUPPRESS)  # test hook
    args = ap.parse_args()

    CURATED = args.curated

    if args.list:
        nodes = read_json(CURATED / "nodes.json", {"nodes": []})
        edges = read_json(CURATED / "edges.json", {"edges": []})
        for n in nodes["nodes"]:
            print(f"{n['type']:<12} {n['id']}")
        for e in edges["edges"]:
            print(f"edge         {e['src']} --{e['rel']}--> {e['dst']}")
        return 0

    if not args.node and not args.edge:
        raise SystemExit("[!] nothing to do: pass --node or --edge (or --list)")

    if args.node:
        do_node(parse_kv(args.node))
    if args.edge:
        do_edge(parse_kv(args.edge))
    if not args.no_rebuild:
        rebuild_with_rollback(args.store_out)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BrokenPipeError:  # e.g. `write_memory.py --list | head`
        raise SystemExit(0)
