#!/usr/bin/env python3
"""Embed only a locally verified commit and explicitly requested release tag."""
import json, os, pathlib, re, subprocess

sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
if not re.fullmatch(r"[0-9a-f]{40}", sha) or sha != os.environ.get("GITHUB_SHA"):
    raise SystemExit("Unexpected source commit")
changed = subprocess.check_output(["git", "diff", "--name-only", "HEAD^", "HEAD"], text=True).splitlines()
tag = ""
if ".github/preview-release.json" in changed:
    marker = json.loads(pathlib.Path(".github/preview-release.json").read_text(encoding="utf-8"))
    candidate = marker.get("tag", "")
    if marker.get("prerelease") is not True or not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+-alpha\.[0-9]+", candidate):
        raise SystemExit("Invalid explicit preview marker")
    tag = candidate
with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as f:
    f.write(f"REMOTE_EX_BUILD_COMMIT={sha}\nREMOTE_EX_BUILD_TAG={tag}\n")
print("Build identity:", tag or "development", sha)
