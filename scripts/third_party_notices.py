#!/usr/bin/env python3
"""Write THIRD-PARTY-NOTICES.md: the license of every crate compiled into the
release binary, with the license texts the crates ship.

Only normal (runtime) dependencies of the x86_64 musl target are included;
build scripts and dev dependencies are not part of the binary.

Usage: python3 scripts/third_party_notices.py  (needs cargo on PATH or in ~/.cargo/bin)
"""
import json
import os
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-unknown-linux-musl"
LICENSE_PREFIXES = ("license", "licence", "copying", "notice")


def cargo():
    return shutil.which("cargo") or os.path.expanduser("~/.cargo/bin/cargo")


def main():
    meta = json.loads(
        subprocess.check_output(
            [cargo(), "metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET],
            cwd=ROOT,
        )
    )
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]

    # Walk normal dependencies only.
    seen, stack = set(), [root]
    while stack:
        node = nodes[stack.pop()]
        for dep in node["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                stack.append(dep["pkg"])

    crates = sorted((pkgs[i] for i in seen), key=lambda p: (p["name"], p["version"]))
    out = [
        "# Third-party notices\n",
        "The `odoo-cli` binary includes the following Rust crates. Each is used",
        "under its own license, reproduced below as shipped by the crate.",
        "`odoo-cli` itself is licensed under AGPL-3.0-only (see `LICENSE`).\n",
        "| Crate | Version | License |",
        "|---|---|---|",
    ]
    for p in crates:
        out.append(f"| {p['name']} | {p['version']} | {p.get('license') or 'see text'} |")
    out.append("")

    for p in crates:
        crate_dir = Path(p["manifest_path"]).parent
        files = sorted(
            f for f in crate_dir.iterdir()
            if f.is_file() and f.name.lower().startswith(LICENSE_PREFIXES)
        )
        out.append(f"## {p['name']} {p['version']}\n")
        out.append(f"License: {p.get('license') or 'see below'}")
        if p.get("repository"):
            out.append(f"Source: {p['repository']}")
        out.append("")
        if not files and p.get("license_file"):
            files = [crate_dir / p["license_file"]]
        if not files:
            out.append(f"(No license file shipped; SPDX expression: {p.get('license')}.)\n")
        for f in files:
            out.append(f"### {f.name}\n")
            out.append("```text")
            out.append(f.read_text(errors="replace").rstrip())
            out.append("```\n")

    dest = ROOT / "THIRD-PARTY-NOTICES.md"
    dest.write_text("\n".join(out) + "\n")
    print(f"{dest}: {len(crates)} crates")


if __name__ == "__main__":
    main()
