#!/usr/bin/env python3
"""test_graph.py — tests for the agentic memory graph + RAG retrieval.

Run from anywhere: python3 harness/memory/test_graph.py
(uses the real repo sources; builds throwaway stores under a temp dir,
plus one read-only check of the canonical store).
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import build_graph  # noqa: E402
import graph_store  # noqa: E402

REPO = build_graph.REPO


class GraphBuildTests(unittest.TestCase):
    """Build correctness: schema, determinism, curated merge."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.out1 = Path(cls.tmp.name) / "g1.json"
        cls.out2 = Path(cls.tmp.name) / "g2.json"
        cls.store1 = build_graph.build(cls.out1)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_schema_valid(self):
        s = self.store1
        self.assertEqual(s["schema_version"], 1)
        self.assertEqual(s["node_count"], len(s["nodes"]))
        self.assertEqual(s["edge_count"], len(s["edges"]))
        graph_store.validate_nodes_edges(s["nodes"], s["edges"])

    def test_expected_node_families_present(self):
        ids = {n["id"] for n in self.store1["nodes"]}
        for expected in (
            "project:multitunnel", "protocol:pstack", "protocol:memory-graph",
            "pm:architecture", "pm:playbook", "pm:contracts", "state:current",
            "decision:D17", "decision:D23", "decision:D24",
            "ledger:s13", "release:v0.7.1", "task:saturation-aware-split",
            "task:e-ui-logo-titlebar-tray", "bug:uneven-traffic-split",
            "component:mt-bond", "commit:f4a26ca",
        ):
            self.assertIn(expected, ids, f"missing node {expected}")

    def test_deterministic_rebuild(self):
        build_graph.build(self.out2)
        self.assertEqual(self.out1.read_bytes(), self.out2.read_bytes(),
                         "rebuild must be byte-identical (no wall-clock)")

    def test_curated_release_edges(self):
        rels = {(e["src"], e["dst"]) for e in self.store1["edges"]}
        for task, rel in (
            ("task:charts-rendering-fix", "release:v0.6.0"),
            ("task:stream-bonding", "release:v0.6.2"),
            ("task:balanced-priority-scheduling", "release:v0.7.0"),
            ("task:status-page-layout", "release:v0.7.1"),
            ("task:saturation-aware-split", "bug:uneven-traffic-split"),
        ):
            self.assertIn((task, rel), rels, f"missing curated edge {task}->{rel}")

    def test_git_evidence_edges(self):
        edges = {(e["src"], e["dst"], e["rel"]) for e in self.store1["edges"]}
        self.assertIn(("release:v0.7.1", "commit:e38a37a", "built_from"), edges,
                      "release tag -> commit edge missing")
        self.assertIn(("commit:f4a26ca", "ledger:s14", "part_of"), edges,
                      "wip(s14) commit -> ledger stage edge missing")


class RetrievalTests(unittest.TestCase):
    """RAG retrieval quality: BM25 ranking + graph expansion."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.store = build_graph.build(Path(cls.tmp.name) / "g.json")

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def _top_ids(self, query: str, k: int = 6, expand: int = 1) -> list[str]:
        res = graph_store.retrieve(self.store, query, k=k, expand=expand)
        self.assertTrue(res, f"no results for {query!r}")
        return [r["node"] for r in res]

    def test_query_saturation(self):
        ids = self._top_ids("traffic split evenly adapter saturates maximum bandwidth")
        self.assertTrue(
            {"task:saturation-aware-split", "bug:uneven-traffic-split"} & set(ids),
            f"saturation query missed: {ids}")

    def test_query_scroll_layout(self):
        ids = self._top_ids("status tab wheel scrolls connection frame graph fills")
        self.assertTrue(
            {"task:status-page-layout", "pm:playbook", "decision:D24"} & set(ids),
            f"scroll query missed: {ids}")

    def test_query_tray(self):
        ids = self._top_ids("tray icon quick settings minimize close")
        self.assertTrue(
            {"task:e-ui-logo-titlebar-tray", "decision:D4"} & set(ids),
            f"tray query missed: {ids}")

    def test_query_protocol(self):
        ids = self._top_ids("memory graph RAG agents retrieval")
        self.assertIn("protocol:memory-graph", ids,
                      f"protocol query missed: {ids}")

    def test_query_release_history(self):
        ids = self._top_ids("v0.7.1 release verified assets")
        self.assertTrue(
            {"release:v0.7.1", "ledger:s13", "task:status-page-layout"} & set(ids),
            f"release query missed: {ids}")

    def test_graph_expansion_adds_neighbors(self):
        res = graph_store.retrieve(
            self.store, "freedom loopback blackhole merge daemon", k=3, expand=1)
        hit_ids = {r["node"] for r in res if r["kind"] == "hit"}
        neighbor_ids = {r["node"] for r in res if r["kind"] == "neighbor"}
        self.assertTrue(neighbor_ids, "expansion returned no neighbors")
        # neighbors must be graph-adjacent to some hit
        adj = graph_store.adjacency(self.store)
        for nb in neighbor_ids:
            self.assertTrue(
                any(other in hit_ids for other, _, _ in adj.get(nb, [])),
                f"neighbor {nb} not adjacent to any hit")

    def test_results_sorted_by_score(self):
        res = graph_store.retrieve(self.store, "carrier scheduler policy", k=6)
        scores = [r["score"] for r in res]
        self.assertEqual(scores, sorted(scores, reverse=True))

    def test_empty_query(self):
        self.assertEqual(graph_store.retrieve(self.store, "zzzqqqxxx", k=3), [])


class CliTests(unittest.TestCase):
    """Agent-facing CLIs work end to end."""

    @classmethod
    def setUpClass(cls):
        # ensure the canonical store exists (auto-built by the query CLI if
        # missing, but build explicitly for a clean signal)
        build_graph.build(build_graph.DEFAULT_OUT)

    def test_query_cli_json(self):
        r = subprocess.run(
            [sys.executable, str(REPO / "query_memory.py"), "zigbuild windows",
             "--json"],
            capture_output=True, text=True, cwd=REPO)
        self.assertEqual(r.returncode, 0, r.stderr)
        data = json.loads(r.stdout)
        self.assertTrue(data["results"])
        self.assertIn("subgraph", data)

    def test_query_cli_prompt_pack(self):
        r = subprocess.run(
            [sys.executable, str(REPO / "query_memory.py"),
             "saturation spill", "--prompt"],
            capture_output=True, text=True, cwd=REPO)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("Retrieved MultiTunnel memory", r.stdout)

    def test_write_cli_roundtrip(self):
        with tempfile.TemporaryDirectory() as td:
            # a realistic curated dir: copy the real one, then write on top
            # (an empty curated dir cannot satisfy edges that reference
            # curated-only nodes like the project hub)
            curated = Path(td) / "curated"
            shutil.copytree(REPO / "harness" / "memory" / "curated", curated)
            store_out = Path(td) / "store.json"
            r = subprocess.run(
                [sys.executable, str(REPO / "harness" / "memory" / "write_memory.py"),
                 "--node", "id=fact:test-probe", "type=fact",
                 "title=Probe fact", "passage=The probe passage is findable.",
                 "--curated", str(curated),
                 "--store-out", str(store_out)],
                capture_output=True, text=True, cwd=REPO)
            self.assertEqual(r.returncode, 0, r.stderr)
            store = graph_store.load_store(store_out)
            ids = {n["id"] for n in store["nodes"]}
            self.assertIn("fact:test-probe", ids)
            # the REAL curated dir must be untouched by the test override
            real = json.loads(
                (REPO / "harness" / "memory" / "curated" / "nodes.json")
                .read_text(encoding="utf-8"))
            self.assertNotIn("fact:test-probe", {n["id"] for n in real["nodes"]})

    def test_write_cli_rejects_bad_type(self):
        with tempfile.TemporaryDirectory() as td:
            curated = Path(td) / "curated"
            curated.mkdir()
            (curated / "nodes.json").write_text('{"nodes": []}', encoding="utf-8")
            (curated / "edges.json").write_text('{"edges": []}', encoding="utf-8")
            r = subprocess.run(
                [sys.executable, str(REPO / "harness" / "memory" / "write_memory.py"),
                 "--node", "id=x:y", "type=bogus", "title=t", "passage=p",
                 "--curated", str(curated),
                 "--store-out", str(Path(td) / "s.json")],
                capture_output=True, text=True, cwd=REPO)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("unknown node type", r.stderr + r.stdout)

    def test_validation_rejects_dangling_edge(self):
        with self.assertRaises(ValueError):
            graph_store.validate_nodes_edges(
                [{"id": "a", "type": "fact", "title": "A", "passages": []}],
                [{"src": "a", "dst": "ghost", "rel": "relates_to"}])


if __name__ == "__main__":
    unittest.main(verbosity=2)
