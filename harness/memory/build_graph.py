#!/usr/bin/env python3
"""build_graph.py — build the MultiTunnel agentic memory graph.

Ingests every memory source into one typed graph store
(`harness/memory/store/memory_graph.json`):

    PROJECT_MEMORY.md   durable sections (architecture / playbook / contracts)
    harness/memory/     STATE.md, DECISIONS.md (D<n>), LEDGER.md (per stage tag),
                        agent-reports/*.md, consensus/*.md
    git                 tags v* (release nodes, built_from edges) + full commit
                        log (commit nodes, part_of stage edges)
    curated/            nodes.json + edges.json — the agent WRITE path
                        (write_memory.py appends here; merged last, wins by id)

Output is deterministic: byte-identical on rebuild while HEAD and sources are
unchanged (no wall-clock in the store; `as_of` comes from git). Run via
`python3 update_memory.py` (which calls this) or directly.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from graph_store import (  # noqa: E402
    MAX_PASSAGE_CHARS,
    SCHEMA_VERSION,
    validate_nodes_edges,
)

REPO = Path(__file__).resolve().parents[2]
MEM = REPO / "harness" / "memory"
DEFAULT_OUT = MEM / "store" / "memory_graph.json"

PM_HEADER_ACTIVITY = "## Recent Progress & Git Activity"


def git(*args: str) -> str:
    r = subprocess.run(["git", "-C", str(REPO), *args],
                       capture_output=True, text=True, check=True,
                       encoding="utf-8")
    return r.stdout


# ---------------------------------------------------------------------------
# Passage helpers
# ---------------------------------------------------------------------------

def clip(text: str) -> str:
    t = " ".join(text.split())
    if len(t) <= MAX_PASSAGE_CHARS:
        return t
    cut = t.rfind(". ", 120, MAX_PASSAGE_CHARS)
    if cut == -1:
        cut = t.rfind(" ", 120, MAX_PASSAGE_CHARS)
    return t[: cut + 1] if cut > 0 else t[:MAX_PASSAGE_CHARS]


def md_passages(text: str) -> list[dict]:
    """Split markdown into passages on blank lines; headings become title
    passages; each passage clipped. Deterministic, order-preserving."""
    out = []
    for block in re.split(r"\n\s*\n", text):
        block = block.strip()
        if not block or block.startswith("```"):
            continue
        kind = "title" if block.startswith("#") else "body"
        out.append({"text": clip(block), "kind": kind})
    return out


# ---------------------------------------------------------------------------
# Source parsers
# ---------------------------------------------------------------------------

def parse_project_memory(nodes: list, edges: list) -> None:
    pm = (REPO / "PROJECT_MEMORY.md").read_text(encoding="utf-8")
    durable = pm.split(PM_HEADER_ACTIVITY)[0]
    sections = {
        "pm:architecture": ("architecture", "Tech Stack & Architecture Rules"),
        "pm:playbook": ("playbook", "Active Playbook & Todo List"),
        "pm:contracts": ("contract", "Verification Contracts"),
    }
    for nid, (ntype, header) in sections.items():
        m = re.search(rf"^## {re.escape(header)}\s*$", durable, re.M)
        if not m:
            continue
        body = durable[m.end():]
        nxt = re.search(r"^## ", body, re.M)
        if nxt:
            body = body[: nxt.start()]
        nodes.append({
            "id": nid, "type": ntype, "title": header,
            "tags": ["durable"],
            "sources": [{"file": "PROJECT_MEMORY.md", "anchor": header}],
            "passages": md_passages(body),
        })
        edges.append({"src": nid, "dst": "project:multitunnel", "rel": "part_of",
                      "evidence": "durable PM section"})


def parse_state(nodes: list, edges: list) -> None:
    f = MEM / "STATE.md"
    if not f.exists():
        return
    text = f.read_text(encoding="utf-8")
    m = re.search(r"Last update:\s*(.+?)\)?\s*$", text, re.M)
    stamp = m.group(1).strip() if m else "unknown"
    nodes.append({
        "id": "state:current", "type": "state",
        "title": f"STATE — current position (last update: {stamp})",
        "tags": ["durable"],
        "sources": [{"file": "harness/memory/STATE.md", "anchor": ""}],
        "passages": md_passages(text),
    })
    edges.append({"src": "state:current", "dst": "project:multitunnel",
                  "rel": "part_of", "evidence": "current state snapshot"})


def parse_decisions(nodes: list, edges: list) -> None:
    f = MEM / "DECISIONS.md"
    if not f.exists():
        return
    text = f.read_text(encoding="utf-8")
    parts = re.split(r"^## (D\d+) — (.+)$", text, flags=re.M)
    # parts: [preamble, id, title, body, id, title, body, ...]
    for i in range(1, len(parts) - 2, 3):
        did, title, body = parts[i], parts[i + 1], parts[i + 2]
        nodes.append({
            "id": f"decision:{did}", "type": "decision",
            "title": clip(f"{did} — {title}"),
            "tags": ["immutable"],
            "sources": [{"file": "harness/memory/DECISIONS.md", "anchor": f"{did}"}],
            "passages": md_passages(body),
        })
        edges.append({"src": f"decision:{did}", "dst": "project:multitunnel",
                      "rel": "part_of", "evidence": "project decision log"})


LEDGER_LINE = re.compile(
    r"^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}Z) \| ([^|]+?) \| (.+)$")
STAGE_TAG = re.compile(r"^s\d+(\.\d+)?$")


def parse_ledger(nodes: list, edges: list, known_releases: set[str]) -> None:
    f = MEM / "LEDGER.md"
    if not f.exists():
        return
    groups: dict[str, dict] = {}
    order: list[str] = []
    for line in f.read_text(encoding="utf-8").splitlines():
        m = LEDGER_LINE.match(line.strip())
        if not m:
            continue
        ts, tag, body = m.group(1), m.group(2).strip(), m.group(3).strip()
        if tag not in groups:
            groups[tag] = {"first": ts, "lines": []}
            order.append(tag)
        groups[tag]["lines"].append(f"{ts} | {body}")
    for tag in order:
        g = groups[tag]
        ntype = "ledger"
        title = (f"Stage {tag} ledger" if STAGE_TAG.match(tag)
                 else f"LEDGER stream: {tag}")
        nodes.append({
            "id": f"ledger:{tag}", "type": ntype, "title": title,
            "tags": [tag, "append-only"],
            "sources": [{"file": "harness/memory/LEDGER.md", "anchor": tag}],
            "passages": [{"text": clip(l), "kind": "body"} for l in g["lines"]],
        })
        edges.append({"src": f"ledger:{tag}", "dst": "project:multitunnel",
                      "rel": "part_of", "evidence": "project ledger"})
        for line in g["lines"]:
            if "RELEASED" not in line:
                continue
            for ver in re.findall(r"\bv\d+\.\d+\.\d+\b", line)[:2]:
                if ver in known_releases:
                    edges.append({
                        "src": f"ledger:{tag}", "dst": f"release:{ver}",
                        "rel": "released_in",
                        "evidence": clip(line),
                    })


def link_commits_to_ledger(nodes: list, edges: list) -> None:
    """Second pass (needs ledger nodes to exist): connect each commit to its
    stage ledger node via the subject's parenthetical label — longest matching
    tag prefix wins ('backup(s11-ROOT-CAUSE)' → ledger:s11-ROOT-CAUSE,
    'backup(s13-close-v0.7.1)' → ledger:s13)."""
    ledger_tags = {n["id"].split(":", 1)[1] for n in nodes
                   if n["id"].startswith("ledger:")}
    for n in nodes:
        if not n["id"].startswith("commit:"):
            continue
        m = re.search(r"\(([^)]+)\)", n["title"])
        if not m:
            continue
        parts = m.group(1).strip().split("-")
        for i in range(len(parts), 0, -1):
            tag = "-".join(parts[:i])
            if tag in ledger_tags:
                edges.append({"src": n["id"], "dst": f"ledger:{tag}",
                              "rel": "part_of",
                              "evidence": f"subject stage label ({tag})"})
                break


def parse_reports(nodes: list, edges: list, sub: str, ntype: str,
                  rel: str) -> None:
    d = MEM / sub
    if not d.is_dir():
        return
    for f in sorted(d.glob("*.md")):
        text = f.read_text(encoding="utf-8")
        hm = re.search(r"^#\s+(.+)$", text, re.M)
        title = hm.group(1).strip() if hm else f.stem
        nodes.append({
            "id": f"{ntype}:{f.stem}", "type": ntype, "title": clip(title),
            "tags": [sub],
            "sources": [{"file": f"harness/memory/{sub}/{f.name}", "anchor": ""}],
            "passages": md_passages(text)[:25],
        })
        sm = re.match(r"^(s\d+(?:\.\d+)?)[-_]", f.stem)
        target = (f"ledger:{sm.group(1)}" if sm and sm.group(1) in
                  {n["id"].split(":", 1)[1] for n in nodes
                   if n["id"].startswith("ledger:")}
                  else "project:multitunnel")
        edges.append({"src": f"{ntype}:{f.stem}", "dst": target, "rel": rel,
                      "evidence": f"filename stage prefix"})


def parse_git(nodes: list, edges: list) -> set[str]:
    # --- tags → releases (resolve full shas now; shorten after the commit
    #     loop so tag edges reference the same short-sha commit ids) --------
    tag_shas: dict[str, str] = {}          # tag -> full commit sha
    for line in git("for-each-ref", "refs/tags",
                    "--format=%(refname:short)|%(objectname)|"
                    "%(*objectname)|%(creatordate:short)").splitlines():
        parts = line.split("|")
        if len(parts) < 4:
            continue
        tag, sha, deref, date = parts[0].strip(), parts[1].strip(), \
            parts[2].strip(), parts[3].strip()
        if not re.match(r"^v\d+\.\d+\.\d+$", tag):
            continue
        tag_shas[tag] = deref if re.match(r"^[0-9a-f]{40}$", deref) else sha
    # --- commits -----------------------------------------------------------
    fmt = "%h%x1f%H%x1f%s%x1f%ad%x1f%b%x1e"
    raw = git("log", "--all", f"--pretty=format:{fmt}", "--date=short")
    short_of: dict[str, str] = {}          # full sha -> short sha
    for rec in raw.split("\x1e"):
        rec = rec.strip("\n")
        if not rec:
            continue
        short, full, subject, date, body = (rec.split("\x1f") + [""] * 5)[:5]
        short_of[full] = short
        passages = [{"text": clip(subject), "kind": "title"}]
        first_para = next((p for p in body.split("\n\n") if p.strip()), "")
        if first_para.strip():
            passages.append({"text": clip(first_para), "kind": "body"})
        nodes.append({
            "id": f"commit:{short}", "type": "commit",
            "title": clip(subject), "tags": [date],
            "sources": [{"file": "git:log", "anchor": short}],
            "passages": passages,
        })
    # --- release nodes + built_from edges (short-sha normalized) ------------
    for tag, full in sorted(tag_shas.items()):
        commit = short_of.get(full, full[:7])
        nodes.append({
            "id": f"release:{tag}", "type": "release",
            "title": f"Release {tag} (tagged {commit})",
            "tags": [tag],
            "sources": [{"file": f"git:tags/{tag}", "anchor": commit}],
            "passages": [{"text": f"Release {tag}: tag points at commit "
                                  f"{commit}.", "kind": "body"}],
        })
        edges.append({"src": f"release:{tag}", "dst": f"commit:{commit}",
                      "rel": "built_from", "evidence": f"git tag {tag}"})
    return set(tag_shas)


def load_curated(nodes: list, edges: list, curated_dir: Path | None = None) -> None:
    """Curated files are the agent WRITE path — merged LAST so they win
    on id collisions (curated nodes override parsed ones)."""
    nodes_by_id = {n["id"]: n for n in nodes}
    cdir = curated_dir or (MEM / "curated")
    for fname, key in (("nodes.json", "nodes"), ("edges.json", "edges")):
        f = cdir / fname
        if not f.exists():
            continue
        data = json.loads(f.read_text(encoding="utf-8"))
        for item in data.get(key, []):
            if key == "nodes":
                nodes_by_id[item["id"]] = item
            else:
                edges.append(item)
    nodes[:] = list(nodes_by_id.values())


# ---------------------------------------------------------------------------
# Assembly
# ---------------------------------------------------------------------------

def build(out_path: Path, curated_dir: Path | None = None) -> dict:
    nodes: list[dict] = []
    edges: list[dict] = []

    releases = parse_git(nodes, edges)
    parse_project_memory(nodes, edges)
    parse_state(nodes, edges)
    parse_decisions(nodes, edges)
    parse_ledger(nodes, edges, releases)
    link_commits_to_ledger(nodes, edges)
    parse_reports(nodes, edges, "agent-reports", "report", "reports_on")
    parse_reports(nodes, edges, "consensus", "consensus", "reports_on")
    load_curated(nodes, edges, curated_dir)

    # dedupe edges (same src/dst/rel keep first — parsed then curated order)
    seen: set[tuple] = set()
    dedup: list[dict] = []
    for e in edges:
        k = (e["src"], e["dst"], e["rel"])
        if k in seen:
            continue
        seen.add(k)
        dedup.append(e)

    nodes.sort(key=lambda n: n["id"])
    dedup.sort(key=lambda e: (e["src"], e["dst"], e["rel"]))
    validate_nodes_edges(nodes, dedup)

    try:
        as_of = git("log", "-1", "--pretty=%h %cI").strip()
    except subprocess.CalledProcessError:
        as_of = "unknown"
    store = {
        "schema_version": SCHEMA_VERSION,
        "as_of": as_of,
        "sources": [
            "PROJECT_MEMORY.md", "harness/memory/STATE.md",
            "harness/memory/DECISIONS.md", "harness/memory/LEDGER.md",
            "harness/memory/agent-reports/", "harness/memory/consensus/",
            "git (tags + full log)", "harness/memory/curated/",
        ],
        "node_count": len(nodes),
        "edge_count": len(dedup),
        "nodes": nodes,
        "edges": dedup,
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps(store, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8")
    return store


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT,
                    help="output store path (default: %(default)s)")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args()
    store = build(args.out)
    if not args.quiet:
        print(f"[✓] memory graph: {store['node_count']} nodes, "
              f"{store['edge_count']} edges → {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
