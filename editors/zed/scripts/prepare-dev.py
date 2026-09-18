#!/usr/bin/env python3
"""Produce a loadable dev extension without committing generated files to Zen."""
from pathlib import Path
import shutil
import subprocess
import json

repo = Path(__file__).resolve().parents[3]
source = repo / "tooling/tree-sitter-zen"
out = repo / "target/zed-dev"
grammar = repo / "target/tree-sitter-zen-dev"
subprocess.run(["tree-sitter", "generate"], cwd=source, check=True)
for name in ["grammar.js", "tree-sitter.json", "package.json", "src"]:
    src, dst = source / name, grammar / name
    dst.parent.mkdir(parents=True, exist_ok=True)
    if src.is_dir():
        shutil.copytree(src, dst, dirs_exist_ok=True)
    else:
        shutil.copy2(src, dst)
subprocess.run(["git", "init", "-q", str(grammar)], check=True)
subprocess.run(["git", "add", "."], cwd=grammar, check=True)
subprocess.run(["git", "-c", "user.name=Zen Dev", "-c", "user.email=dev@localhost",
                "-c", "commit.gpgsign=false", "commit", "--allow-empty", "-qm", "Local grammar snapshot"], cwd=grammar, check=True)
rev = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=grammar, text=True).strip()
extension = repo / "editors/zed"
for name in ["Cargo.toml", "Cargo.lock", "src", "languages"]:
    src, dst = extension / name, out / name
    dst.parent.mkdir(parents=True, exist_ok=True)
    if src.is_dir():
        shutil.copytree(src, dst, dirs_exist_ok=True)
    else:
        shutil.copy2(src, dst)
manifest = (extension / "extension.toml").read_text()
manifest = manifest.replace('"file:///tmp/zen-tree-sitter-dev"', json.dumps(grammar.as_uri()))
manifest = manifest.replace('rev = "HEAD"', 'rev = ' + json.dumps(rev))
(out / "extension.toml").write_text(manifest)
print(f"Install Dev Extension in Zed: {out}")
