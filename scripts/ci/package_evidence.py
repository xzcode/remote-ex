#!/usr/bin/env python3
"""Emit evidence for exact native packages, after all native smoke checks.
Only repository build identity and package sizes/hashes are recorded.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re

NAMES = {
    "windows": ("Remote-EX-dev-Windows-x64.zip", "Remote-EX-dev-Windows-x64-Setup.exe"),
    "macos": ("Remote-EX-dev-macOS-arm64.tar.gz", "Remote-EX-dev-macOS-arm64.dmg"),
    "linux": ("Remote-EX-dev-Linux-x64.tar.gz", "Remote-EX-dev-Linux-x64.deb"),
}
ARCH = {"windows": "X64", "macos": "ARM64", "linux": "X64"}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", choices=NAMES, required=True)
    parser.add_argument("--dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    commit = os.environ["GITHUB_SHA"]
    tag = os.environ.get("REMOTE_EX_BUILD_TAG", "")
    run = os.environ["GITHUB_RUN_ID"]
    arch = os.environ["RUNNER_ARCH"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or not re.fullmatch(r"[0-9]+", run):
        raise ValueError("Invalid native build identity")
    if tag and not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+-alpha\.[0-9]+", tag):
        raise ValueError("Invalid native release tag")
    if arch != ARCH[args.platform]:
        raise ValueError("Unexpected native architecture")
    rows = []
    for name in NAMES[args.platform]:
        path = args.dir / name
        if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= 1024*1024*1024:
            raise ValueError("Invalid expected package: " + name)
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        rows.append({"name": name, "size": path.stat().st_size, "sha256": digest})
    evidence = {"schema": 1, "commit": commit, "tag": tag, "run_id": run,
                "platform": args.platform, "architecture": arch, "packages": rows}
    output = args.dir / "PACKAGE_EVIDENCE.json"
    output.write_text(json.dumps(evidence, indent=2)+"\n", encoding="utf-8")
    print(f"PASS native package evidence: {args.platform}, {commit}, {len(rows)} files")

if __name__ == "__main__":
    main()
