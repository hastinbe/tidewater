"""Regenerate the Cargo dependency table of THIRD_PARTY_NOTICES.md.

    python3 tools/gen_notices.py

Reads `cargo metadata --locked` and rewrites the block between the GENERATED markers. Workspace crates are left out.
"""
import json
import os
import subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PATH = os.path.join(ROOT, "THIRD_PARTY_NOTICES.md")
BEGIN, END = "<!-- BEGIN GENERATED -->", "<!-- END GENERATED -->"

meta = json.loads(subprocess.run(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT,
                                 capture_output=True, text=True, check=True).stdout)
members = set(meta["workspace_members"])
rows = sorted({(p["name"], p["version"], p["license"] or "see crate", p["repository"] or "")
               for p in meta["packages"] if p["id"] not in members})
lines = ["| Crate | Version | Licence |", "|---|---|---|"]
for name, ver, lic, repo in rows:
    label = f"[{name}]({repo})" if repo else name
    lines.append(f"| {label} | {ver} | {lic.replace('|', '/')} |")
text = open(PATH, encoding="utf-8").read()
head, rest = text.split(BEGIN)
tail = rest.split(END)[1]
open(PATH, "w", encoding="utf-8").write(f"{head}{BEGIN}\n" + "\n".join(lines) + f"\n{END}{tail}")
print(f"{len(rows)} crates")
