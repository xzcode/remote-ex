#!/usr/bin/env python3
"""Record resolved dependency versions from local build lockfiles.

This is an inventory of resolution inputs, not a full SBOM, binary inclusion
proof, license compliance finding or dependency vulnerability scan. No network.
Never include registry URLs, credentials, machine paths or environment dumps.
"""
import argparse
import json
import os
import pathlib
import re
import tomllib

def cargo(path, scope):
    doc = tomllib.loads(path.read_text(encoding="utf-8"))
    result = []
    for p in doc.get("package", []):
        source = p.get("source", "")
        checksum = p.get("checksum", "")
        item = {"scope": scope, "ecosystem": "cargo", "name": p["name"], "version": p["version"],
                "source": "registry" if source.startswith("registry+") else "git" if source.startswith("git+") else "local"}
        if re.fullmatch(r"[0-9a-f]{64}", checksum):
            item["sha256"] = checksum
        result.append(item)
    if not result:
        raise ValueError(f"No resolved packages in {scope}")
    return result

def npm(path):
    doc = json.loads(path.read_text(encoding="utf-8"))
    rows = []
    for key, p in doc.get("packages", {}).items():
        if not key or not p.get("version"):
            continue
        name = p.get("name") or key.rsplit("node_modules/", 1)[-1]
        item = {"scope":"frontend", "ecosystem":"npm", "name":name, "version":p["version"],
                "dev":bool(p.get("dev")), "optional":bool(p.get("optional"))}
        if isinstance(p.get("integrity"), str) and re.fullmatch(r"sha(?:256|384|512)-[A-Za-z0-9+/=]+",p["integrity"]):
            item["integrity"] = p["integrity"]
        if isinstance(p.get("license"),str) and len(p["license"])<=128:
            item["declared_license"] = p["license"]
        rows.append(item)
    if not rows:
        raise ValueError("No resolved frontend packages")
    return rows

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--out",required=True,type=pathlib.Path)
    p.add_argument("--platform",required=True,choices=["windows","macos","linux"])
    args = p.parse_args()
    sha = os.environ.get("GITHUB_SHA","")
    if not re.fullmatch(r"[0-9a-f]{40}",sha):
        raise ValueError("Invalid source commit")
    rows = []
    for scope, path in [
        ("workspace", "Cargo.lock"), ("media", "crates/media/Cargo.lock"),
        ("desktop", "apps/desktop/src-tauri/Cargo.lock")
    ]:
        rows.extend(cargo(pathlib.Path(path),scope))
    rows.extend(npm(pathlib.Path("apps/desktop/package-lock.json")))
    rows.sort(key=lambda r:(r["scope"],r["ecosystem"],r["name"],r["version"]))
    report = {"schema":1,"commit":sha,"platform":args.platform,"architecture":os.environ.get("RUNNER_ARCH","unknown"),
              "external_runtime":"GStreamer (not bundled)","scope":"resolved build inputs; includes development and target-specific dependencies",
              "license_review":"incomplete","packages":rows}
    args.out.parent.mkdir(parents=True,exist_ok=True)
    args.out.write_text(json.dumps(report,indent=2,ensure_ascii=True)+"\n",encoding="utf-8")
    print(f"Recorded {len(rows)} resolved dependency entries ({args.platform})")
if __name__ == "__main__":
    main()
