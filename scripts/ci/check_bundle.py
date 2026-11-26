#!/usr/bin/env python3
"""Stamp and verify fixed local component names, and smoke-test the runtime.
No download, install, capture, input, network or device registration.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import subprocess

NAMES = {
    "windows": ["lan_view.exe", "lan_diagnostic.exe", "lan_control.exe", "routed_diagnostic.exe", "routed_control.exe"],
    "macos": ["lan_view", "lan_diagnostic", "lan_control", "routed_diagnostic", "routed_control", "remote-ex-mac-audio"],
    "linux": ["lan_view", "lan_diagnostic", "routed_x11_view", "routed_diagnostic"],
}
PROBE = {"windows": "routed_control.exe", "macos": "routed_control", "linux": "routed_x11_view"}
ARCH = {"X64": "x86_64", "ARM64": "aarch64", "x86_64": "x86_64", "aarch64": "aarch64"}

def entry(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"Expected regular component: {path.name}")
    size = path.stat().st_size
    if not 0 < size <= 512 * 1024 * 1024:
        raise ValueError(f"Invalid component size: {path.name}")
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"name": path.name, "size": size, "sha256": digest}

def manifest(directory, platform, arch, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Invalid source commit")
    return {"schema": 1, "platform": platform, "arch": ARCH[arch], "commit": commit,
            "files": [entry(directory / n) for n in NAMES[platform]]}

def probe(directory, platform):
    exe = directory / PROBE[platform]
    result = subprocess.run([str(exe.resolve()), "--runtime-check"],
                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, timeout=15, check=True)
    if len(result.stdout) > 8192:
        raise ValueError("Oversized local probe response")
    p = json.loads(result.stdout)
    if p["schema"] != 1 or not p["initialized"] or not p["synthetic_video"]:
        raise ValueError("Synthetic codec probe failed")
    if p["version"][0] != 1 or p["version"][1] < 22:
        raise ValueError("Unsupported GStreamer version")
    optional = {"autovideosink", "pipewiresrc", "wasapi2src"}
    if platform == "linux":
        optional.remove("pipewiresrc")
    missing = set(p["missing"]) - optional
    if missing:
        raise ValueError(f"Missing required runtime elements: {sorted(missing)}")
    print("PASS local synthetic VP8 and media plugin discovery", p["version"][:3])

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["stamp", "verify", "probe"])
    parser.add_argument("--dir", type=pathlib.Path, required=True)
    parser.add_argument("--platform", choices=NAMES, required=True)
    parser.add_argument("--arch", choices=ARCH, default=os.environ.get("RUNNER_ARCH", "X64"))
    parser.add_argument("--commit", default=os.environ.get("GITHUB_SHA", ""))
    args = parser.parse_args()
    directory = args.dir.resolve()
    if args.mode == "probe":
        probe(directory, args.platform)
        return
    path = directory / "sidecars.json"
    if args.platform == "macos" and directory.name == "MacOS" and directory.parent.name == "Contents":
        path = directory.parent / "Resources" / "sidecars.json"
    expected = manifest(directory, args.platform, args.arch, args.commit)
    if args.mode == "stamp":
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("x", encoding="utf-8") as f:
            json.dump(expected, f, indent=2)
            f.write("\n")
    else:
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 16384:
            raise ValueError("Missing or invalid sidecar manifest")
        if json.loads(path.read_text(encoding="utf-8")) != expected:
            raise ValueError("Packaged component manifest differs from staged files")
    print(f"PASS {args.mode}: {args.platform} {expected['arch']} {len(expected['files'])} components")

if __name__ == "__main__":
    main()
