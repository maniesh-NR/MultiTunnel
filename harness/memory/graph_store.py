#!/usr/bin/env python3
"""graph_store.py — MultiTunnel agentic memory graph + RAG retrieval library.

Zero-dependency (stdlib only). The store is a single deterministic JSON file
(`harness/memory/store/memory_graph.json`): typed nodes with retrieval
passages + typed evidence edges. Retrieval is hybrid: BM25 lexical ranking
over passages, then graph-neighborhood expansion (the "RAG" retrieval layer —
the generation layer is whichever agent consumes the emitted context pack).

Stable agent-facing surface (do not break):
    load_store(path)            -> dict
    retrieve(store, query, k)   -> list[dict]   (scored, deterministic)
    format_human(results)       -> str
    format_prompt(query, res)   -> str   (paste-ready RAG context pack)
    NODE_TYPES / EDGE_RELS      -> whitelists (validation)
"""

from __future__ import annotations

import json
import math
import re
from pathlib import Path

# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------

SCHEMA_VERSION = 1

#: Whitelist of node types. Anything else is rejected at write time.
NODE_TYPES = (
    "project",      # the root hub (project:multitunnel)
    "architecture", # durable architecture rules (pm: Tech Stack section)
    "playbook",     # current stage + todo (pm: Active Playbook section)
    "contract",     # verification contracts (pm + harness/CONTRACTS-*)
    "state",        # harness/memory/STATE.md snapshot
    "decision",     # harness/memory/DECISIONS.md D<n> entries (immutable)
    "ledger",       # harness/memory/LEDGER.md, grouped by event tag (stage)
    "release",      # git tags v* + curated release facts
    "commit",       # git history (one node per commit)
    "task",         # user-facing task/feature work items
    "bug",          # defects (open, fixed, or regression)
    "component",    # crates / binaries / subsystems
    "protocol",     # working agreements (pstack, this memory graph)
    "report",       # harness/memory/agent-reports/*.md
    "consensus",    # harness/memory/consensus/*.md
    "fact",         # standalone durable facts
)

#: Whitelist of edge relations (src --rel--> dst).
EDGE_RELS = (
    "part_of",      # structural containment (component→project, commit→stage)
    "implements",   # task/commit implements a decision or design
    "fixes",        # task/commit fixes a bug
    "fixed_by",     # inverse of fixes (bug→task), used sparingly
    "caused_by",    # bug caused by another node
    "supersedes",   # node replaces another node
    "relates_to",   # generic semantic link (evidence required)
    "released_in",  # task/change contained in a release
    "built_from",   # release built from a commit (git tag evidence)
    "verified_by",  # claim verified by a test/rig/CI node
    "decided_in",   # design choice recorded as decision D<n>
    "documents",    # node documents another node
    "reports_on",   # report/consensus file covers a stage/task
    "next_stage",   # ordering between stage ledger nodes
)

# BM25 parameters (standard Robertson defaults).
BM25_K1 = 1.5
BM25_B = 0.75
#: Title/heading passages outscore body passages by this factor.
TITLE_BOOST = 1.5
#: Score decay per expansion hop for graph-neighbor results.
EXPAND_DECAY = 0.5
#: Passages longer than this are truncated at the last sentence boundary.
MAX_PASSAGE_CHARS = 1600


# ---------------------------------------------------------------------------
# Loading / validation
# ---------------------------------------------------------------------------

def load_store(path: str | Path) -> dict:
    """Load and minimally validate a store file."""
    p = Path(path)
    data = json.loads(p.read_text(encoding="utf-8"))
    if data.get("schema_version") != SCHEMA_VERSION:
        raise ValueError(
            f"store {p}: schema_version {data.get('schema_version')!r} "
            f"!= expected {SCHEMA_VERSION}"
        )
    return data


def validate_nodes_edges(nodes: list[dict], edges: list[dict]) -> None:
    """Raise ValueError on any schema violation (unknown type/rel, bad shape)."""
    seen = set()
    for n in nodes:
        nid = n.get("id")
        if not nid or not isinstance(nid, str):
            raise ValueError(f"node without string id: {n!r:.120}")
        if nid in seen:
            raise ValueError(f"duplicate node id: {nid}")
        seen.add(nid)
        if n.get("type") not in NODE_TYPES:
            raise ValueError(f"node {nid}: unknown type {n.get('type')!r}")
        if not n.get("title"):
            raise ValueError(f"node {nid}: empty title")
        for ps in n.get("passages", []):
            if not ps.get("text"):
                raise ValueError(f"node {nid}: empty passage text")
    for e in edges:
        if e.get("src") not in seen or e.get("dst") not in seen:
            raise ValueError(
                f"edge {e.get('src')!r} -{e.get('rel')!r}-> {e.get('dst')!r}: "
                "endpoint not a known node"
            )
        if e.get("rel") not in EDGE_RELS:
            raise ValueError(f"edge {e.get('src')}->{e.get('dst')}: "
                             f"unknown rel {e.get('rel')!r}")


# ---------------------------------------------------------------------------
# Tokenization + BM25
# ---------------------------------------------------------------------------

def tokenize(text: str) -> list[str]:
    """Lowercase alphanumeric tokens (versions survive as pieces: v0.7.1 →
    v0, 7, 1 — consistent between index and query, so version lookups work)."""
    return re.findall(r"[a-z0-9]+", text.lower())


class Bm25Index:
    """BM25 over (node_id, kind, text) passages. Deterministic."""

    def __init__(self, passages: list[dict]):
        # passages: [{"node": id, "kind": "title"|"body", "text": str}]
        self.docs = []
        for ps in passages:
            toks = tokenize(ps["text"])
            if not toks:
                continue
            self.docs.append({
                "node": ps["node"],
                "kind": ps.get("kind", "body"),
                "text": ps["text"],
                "tokens": toks,
                "len": len(toks),
            })
        self.n = len(self.docs)
        self.avgdl = (sum(d["len"] for d in self.docs) / self.n) if self.n else 0.0
        self.df: dict[str, int] = {}
        for d in self.docs:
            for t in set(d["tokens"]):
                self.df[t] = self.df.get(t, 0) + 1

    def _idf(self, term: str) -> float:
        df = self.df.get(term, 0)
        if df == 0:
            return 0.0
        return math.log(1.0 + (self.n - df + 0.5) / (df + 0.5))

    def score(self, query: str) -> dict[tuple[str, str], float]:
        """Return {(node_id, kind): score} for the query (title-boosted)."""
        terms = tokenize(query)
        out: dict[tuple[str, str], float] = {}
        for d in self.docs:
            s = 0.0
            tf_map: dict[str, int] = {}
            for t in d["tokens"]:
                tf_map[t] = tf_map.get(t, 0) + 1
            for t in set(terms):
                tf = tf_map.get(t, 0)
                if tf == 0:
                    continue
                idf = self._idf(t)
                denom = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * d["len"] / self.avgdl)
                s += idf * tf * (BM25_K1 + 1.0) / denom
            if s <= 0.0:
                continue
            boost = TITLE_BOOST if d["kind"] == "title" else 1.0
            key = (d["node"], d["kind"])
            out[key] = max(out.get(key, 0.0), s * boost)
        return out


def build_index(store: dict) -> Bm25Index:
    """Index every node's title (as a title passage) + body passages."""
    passages = []
    for n in store["nodes"]:
        passages.append({"node": n["id"], "kind": "title", "text": n["title"]})
        for ps in n.get("passages", []):
            passages.append({
                "node": n["id"],
                "kind": ps.get("kind", "body"),
                "text": ps["text"],
            })
    return Bm25Index(passages)


# ---------------------------------------------------------------------------
# Graph expansion
# ---------------------------------------------------------------------------

def adjacency(store: dict) -> dict[str, list[tuple[str, str, str]]]:
    """node_id -> [(neighbor_id, rel, direction)] (both directions)."""
    adj: dict[str, list[tuple[str, str, str]]] = {}
    for e in store["edges"]:
        adj.setdefault(e["src"], []).append((e["dst"], e["rel"], "out"))
        adj.setdefault(e["dst"], []).append((e["src"], e["rel"], "in"))
    return adj


def induced_subgraph(store: dict, node_ids: set[str]) -> list[dict]:
    """Edges whose BOTH endpoints are in node_ids (for result context)."""
    return [
        {"src": e["src"], "dst": e["dst"], "rel": e["rel"]}
        for e in store["edges"]
        if e["src"] in node_ids and e["dst"] in node_ids
    ]


# ---------------------------------------------------------------------------
# Retrieval (BM25 + graph expansion) — the RAG retrieval layer
# ---------------------------------------------------------------------------

def retrieve(store: dict, query: str, k: int = 5, expand: int = 1) -> list[dict]:
    """Hybrid retrieval. Deterministic ordering: (-score, node_id, kind).

    Returns a list of result dicts:
        node, type, title, kind ("hit"/"neighbor"), hop, score, text,
        sources, via (for neighbors: "<rel> <direction> <from-node>")
    """
    index = build_index(store)
    scores = index.score(query)
    if not scores:
        return []
    by_node: dict[str, float] = {}
    best_kind: dict[str, tuple[float, str]] = {}
    for (nid, kind), s in scores.items():
        by_node[nid] = max(by_node.get(nid, 0.0), s)
        cur = best_kind.get(nid)
        if cur is None or s > cur[0]:
            best_kind[nid] = (s, kind)

    ranked = sorted(by_node.items(), key=lambda kv: (-kv[1], kv[0]))
    top = ranked[: max(k, 1)]

    nodes = {n["id"]: n for n in store["nodes"]}
    adj = adjacency(store)
    results: list[dict] = []

    def add(nid: str, kind: str, hop: int, score: float, via: str | None):
        n = nodes[nid]
        results.append({
            "node": nid,
            "type": n["type"],
            "title": n["title"],
            "kind": kind,
            "hop": hop,
            "score": round(score, 4),
            "text": _best_text(n, query),
            "sources": [f"{s['file']}#{s.get('anchor', '')}".rstrip("#")
                        for s in n.get("sources", [])],
            "via": via,
        })

    for nid, s in top:
        add(nid, "hit", 0, s, None)

    if expand > 0:
        seen = {nid for nid, _ in top}
        frontier = {nid: (0, s) for nid, s in top}
        for hop in range(1, expand + 1):
            nxt: dict[str, tuple[int, float]] = {}
            for nid, (_, base) in frontier.items():
                for nb, rel, direction in sorted(adj.get(nid, [])):
                    if nb in seen:
                        continue
                    # edge label direction: out => src --rel--> dst
                    if direction == "out":
                        via = f"--{rel}--> {nid}"
                    else:
                        via = f"<--{rel}-- {nid}"
                    sc = base * (EXPAND_DECAY ** hop)
                    if nb not in nxt or sc > nxt[nb][1]:
                        nxt[nb] = (hop, sc)
            for nb, (h, sc) in sorted(nxt.items()):
                add(nb, "neighbor", h, sc, None)
                # record how it connected (set via from an actual edge)
                results[-1]["via"] = _first_via(adj, nb, seen)
                seen.add(nb)
            frontier = nxt
            if not frontier:
                break

    results.sort(key=lambda r: (-r["score"], r["node"], r["hop"]))
    return results[: max(k, 1) * (2 if expand > 0 else 1)]


def _first_via(adj: dict, nb: str, seen: set[str]) -> str | None:
    for other, rel, direction in sorted(adj.get(nb, [])):
        if other in seen:
            if direction == "out":
                return f"{nb} --{rel}--> {other}"
            return f"{other} --{rel}--> {nb}"
    return None


def _best_text(node: dict, query: str) -> str:
    """Passage of this node with the most query-token overlap (deterministic)."""
    q = set(tokenize(query))
    best, best_score = None, -1
    for ps in node.get("passages", []):
        toks = set(tokenize(ps["text"]))
        sc = len(q & toks)
        if sc > best_score or (sc == best_score and best is None):
            best, best_score = ps["text"], sc
    if best is None:
        best = node["title"]
    if len(best) > 420:
        cut = best.rfind(". ", 0, 420)
        best = (best[: cut + 1] if cut > 120 else best[:417]) + "…"
    return best


# ---------------------------------------------------------------------------
# Output formatting
# ---------------------------------------------------------------------------

def format_human(query: str, results: list[dict]) -> str:
    lines = [f'query: "{query}" — {len(results)} passages '
             f"(BM25 + graph expansion)"]
    for i, r in enumerate(results, 1):
        head = f"[{i}] {r['score']:>7.2f}  {r['type']:<12} {r['node']}"
        if r["kind"] == "neighbor":
            head += f"  (neighbor hop {r['hop']})"
        lines.append(head)
        lines.append(f'    "{r["text"]}"')
        if r["sources"]:
            lines.append(f"    src: {', '.join(r['sources'][:3])}")
        if r["via"]:
            lines.append(f"    via: {r['via']}")
    return "\n".join(lines)


def format_prompt(query: str, results: list[dict], store: dict) -> str:
    """Paste-ready RAG context pack for any consuming agent."""
    out = [
        f'### Retrieved MultiTunnel memory for: "{query}"',
        "(BM25 + graph-neighborhood retrieval over the project memory graph.",
        "Each passage cites its source file; treat it as retrieved evidence —",
        "re-read the cited file before acting on specifics.)",
        "",
    ]
    for i, r in enumerate(results, 1):
        out.append(f"[{i}] ({r['type']}: {r['node']}, score {r['score']})")
        out.append(f"{r['text']}")
        if r["sources"]:
            out.append(f"source: {', '.join(r['sources'])}")
        if r["via"]:
            out.append(f"graph link: {r['via']}")
        out.append("")
    ids = {r["node"] for r in results}
    sub = induced_subgraph(store, ids)
    if sub:
        out.append("Graph links among the results above:")
        for e in sub:
            out.append(f"  {e['src']} --{e['rel']}--> {e['dst']}")
    return "\n".join(out)
