#!/usr/bin/env python3
"""Regenerate THIRD_PARTY_NOTICES.md from Cargo.lock + registry metadata.

Reads every external package in Cargo.lock, resolves its license from the
local cargo registry (extracted Cargo.toml in registry/src, or the .crate
tarball in registry/cache), falls back to the crates.io API, then emits a
grouped-by-license manifest as JSON on stdout. Workspace members (path
deps) are excluded.
"""

import json
import re
import sys
import tarfile
import urllib.request
from pathlib import Path

REPO = Path("/home/z/my-project/MultiTunnel")
LOCK = REPO / "Cargo.lock"
REGISTRY = Path("/home/z/my-project/tools/cargo/registry")


def lock_packages():
    """[(name, version)] for every package with a crates.io source."""
    out = []
    block = {}
    for line in LOCK.read_text().splitlines() + ["[[package]]"]:
        s = line.strip()
        if s == "[[package]]":
            if block.get("source", "").startswith("registry+"):
                out.append((block["name"], block["version"]))
            block = {}
        elif s.startswith("name = "):
            block["name"] = s.split("= ", 1)[1].strip('"')
        elif s.startswith("version = "):
            block["version"] = s.split("= ", 1)[1].strip('"')
        elif s.startswith("source = "):
            block["source"] = s.split("= ", 1)[1].strip('"')
    return sorted(out)


def license_from_src_dir(name, version):
    for src_root in REGISTRY.glob("src/index.crates.io-*"):
        cand = src_root / f"{name}-{version}" / "Cargo.toml"
        if cand.exists():
            m = re.search(r'(?m)^license\s*=\s*"([^"]+)"', cand.read_text(errors="replace"))
            if m:
                return m.group(1)
    return None


def license_from_crate_tarball(name, version):
    for cache_root in REGISTRY.glob("cache/index.crates.io-*"):
        cand = cache_root / f"{name}-{version}.crate"
        if cand.exists():
            try:
                with tarfile.open(cand, "r:gz") as tf:
                    inner = f"{name}-{version}/Cargo.toml"
                    f = tf.extractfile(inner)
                    if f:
                        m = re.search(rb'(?m)^license\s*=\s*"([^"]+)"', f.read())
                        if m:
                            return m.group(1).decode()
            except (tarfile.TarError, OSError):
                pass
    return None


def license_from_api(name, version):
    url = f"https://crates.io/api/v1/crates/{name}/{version}"
    req = urllib.request.Request(url, headers={"User-Agent": "multitunnel-notices/0.3"})
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            lic = json.load(r).get("version", {}).get("license")
            return lic or None
    except Exception as e:
        print(f"API FAILED {name} {version}: {e}", file=sys.stderr)
        return None


def main():
    pkgs = lock_packages()
    print(f"# {len(pkgs)} external packages", file=sys.stderr)
    resolved, missing = {}, []
    for name, version in pkgs:
        lic = license_from_src_dir(name, version)
        if not lic:
            lic = license_from_crate_tarball(name, version)
        if not lic:
            lic = license_from_api(name, version)
        if lic:
            resolved.setdefault(lic, []).append(f"{name} {version}")
        else:
            missing.append(f"{name} {version}")
    print(json.dumps(resolved, indent=1, sort_keys=True))
    if missing:
        print("MISSING LICENSE:", missing, file=sys.stderr)


if __name__ == "__main__":
    main()
