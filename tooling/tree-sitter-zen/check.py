#!/usr/bin/env python3
"""Generate, test syntax/recovery, and compile all Zed queries."""
from pathlib import Path
import os
import subprocess

root = Path(__file__).resolve().parent
repo = root.parent.parent
cache = repo / "target/tree-sitter-cache"
env = {**os.environ, "XDG_CACHE_HOME": str(cache)}

def run(*args):
    subprocess.run(args, cwd=root, env=env, check=True)

run("tree-sitter", "generate")
run("tree-sitter", "test")
fixtures = sorted((repo / "tests/fixtures/valid").glob("*.zen"))
run("tree-sitter", "parse", "--quiet", *(str(p) for p in fixtures))
for query in sorted((repo / "editors/zed/languages/zen").glob("*.scm")):
    subprocess.run(["tree-sitter", "query", str(query), str(fixtures[0])],
                   cwd=root, env=env, check=True, stdout=subprocess.DEVNULL)
print("All Zen corpus tests, compiler fixtures, and Zed queries passed.")
