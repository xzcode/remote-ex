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

# Only local registry/cache metadata corresponding to locked package name and
# version is consulted. Never infer a license from a package's name.
CARGO_HOME = pathlib.Path(os.environ.get("CARGO_HOME", str(pathlib.Path.home()/".cargo")))
REGISTRY = CARGO_HOME/"registry"/"src"
REGISTRY_DIRS = list(REGISTRY.glob("*"))[:32] if REGISTRY.is_dir() else []
FRONTEND = pathlib.Path("apps/desktop").resolve()

def declared(value):
    if isinstance(value,str) and 0 < len(value) <= 256 and not any(ord(c)<32 for c in value):
        return value
    return None

def cargo_license(name,version,source):
    if source!="registry":
        return {"license_evidence":"local_project_license_pending" if source=="local" else "unreviewed_git_dependency"}
    if not re.fullmatch(r"[A-Za-z0-9_-]+",name) or not re.fullmatch(r"[A-Za-z0-9.+_-]+",version):
        return {"license_evidence":"unavailable"}
    matches=[]
    for directory in REGISTRY_DIRS:
        path=directory/(name+"-"+version)/"Cargo.toml"
        try:
            if path.stat().st_size>1024*1024:continue
            pkg=tomllib.loads(path.read_text(encoding="utf-8")).get("package",{})
            if pkg.get("name")!=name or pkg.get("version")!=version:continue
            expression=declared(pkg.get("license"))
            license_file=pkg.get("license-file")
            has_file=False
            if isinstance(license_file,str) and license_file:
                candidate=(path.parent/license_file).resolve()
                has_file=candidate.is_relative_to(path.parent.resolve()) and candidate.is_file()
            matches.append((expression,has_file))
        except (OSError,ValueError,TypeError,tomllib.TOMLDecodeError):
            continue
    if not matches:return {"license_evidence":"not_cached_for_this_build"}
    if len(set(matches))!=1:return {"license_evidence":"conflicting_cached_manifests"}
    expression,has_file=matches[0]
    row={"license_evidence":"cached_registry_manifest","license_file_present":has_file}
    if expression:row["declared_license"]=expression
    elif not has_file:row["license_evidence"]="manifest_has_no_license_declaration"
    return row

def npm_license(key,p):
    value=declared(p.get("license"))
    if value:return {"declared_license":value,"license_evidence":"npm_lockfile"}
    try:
        path=(FRONTEND/key/"package.json").resolve()
        if not path.is_relative_to(FRONTEND/"node_modules") or path.stat().st_size>1024*1024:
            return {"license_evidence":"not_cached_for_this_build"}
        pkg=json.loads(path.read_text(encoding="utf-8"))
        if pkg.get("version")!=p.get("version"):return {"license_evidence":"unavailable"}
        value=declared(pkg.get("license"))
        if value:return {"declared_license":value,"license_evidence":"installed_npm_manifest"}
    except (OSError,ValueError,TypeError):
        pass
    return {"license_evidence":"unavailable"}

def review_flag(item):
    expression=item.get("declared_license","")
    if not expression:return "missing_or_file_only"
    if re.search(r"(^|[^A-Za-z])(A?GPL|LGPL|MPL|EPL|CDDL|SSPL|OSL)-",expression,re.I):
        return "copyleft_or_reciprocal_review"
    if re.search(r"LicenseRef|SEE LICENSE|UNLICENSED|Proprietary|NonCommercial",expression,re.I):
        return "special_terms_review"
    return "declared_review_pending"

def cargo(path, scope):
    doc = tomllib.loads(path.read_text(encoding="utf-8"))
    result = []
    for p in doc.get("package", []):
        source = p.get("source", "")
        checksum = p.get("checksum", "")
        item = {"scope": scope, "ecosystem": "cargo", "name": p["name"], "version": p["version"],
                "source": "registry" if source.startswith("registry+") else "git" if source.startswith("git+") else "local"}
        item.update(cargo_license(p["name"],p["version"],item["source"]))
        item["license_review_flag"]=review_flag(item)
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
        item.update(npm_license(key,p))
        item["license_review_flag"]=review_flag(item)
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
    flags={}
    for item in rows:
        flag=item["license_review_flag"];flags[flag]=flags.get(flag,0)+1
    report = {"schema":1,"commit":sha,"platform":args.platform,"architecture":os.environ.get("RUNNER_ARCH","unknown"),
              "external_runtime":"GStreamer (not bundled)","scope":"resolved build inputs; includes development and target-specific dependencies",
              "license_review":"incomplete","license_evidence_summary":{"declared":sum("declared_license" in p for p in rows),"total":len(rows),"review_flags":flags},"packages":rows}
    args.out.parent.mkdir(parents=True,exist_ok=True)
    args.out.write_text(json.dumps(report,indent=2,ensure_ascii=True)+"\n",encoding="utf-8")
    print(f"Recorded {len(rows)} resolved dependency entries ({args.platform})")
if __name__ == "__main__":
    main()
