#!/usr/bin/env python3
"""M0 synthetic media diagnostic; never captures a user's screen or opens a network listener."""
import argparse
import json
import platform
import shutil
import subprocess
import sys
import time

def run(args, timeout):
    try:
        r = subprocess.run(args, capture_output=True, text=True, errors="replace", timeout=timeout)
        return {"code": r.returncode, "output": (r.stdout + r.stderr)[-6000:]}
    except (OSError, subprocess.TimeoutExpired) as e:
        return {"code": 1, "output": str(e)}

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--preview", action="store_true", help="display synthetic test pattern locally")
    args = p.parse_args()
    tools = {name: shutil.which(name) for name in ("gst-launch-1.0", "gst-inspect-1.0")}
    report = {"platform": platform.platform(), "tools": tools, "scope": "synthetic local encode/decode only"}
    if not all(tools.values()):
        report["status"] = "blocked_missing_gstreamer"
        print(json.dumps(report, indent=2))
        return 2
    inspect = tools["gst-inspect-1.0"]
    launch = tools["gst-launch-1.0"]
    report["version"] = run([inspect, "--version"], 10)
    elements = ["videotestsrc", "videoconvert", "vp8enc", "vp8dec", "fakesink", "webrtcbin",
                "rtpvp8pay", "rtpvp8depay", "nicesrc", "dtlssrtpenc"]
    if args.preview:
        elements.append("autovideosink")
    report["elements"] = {e: run([inspect, e], 10)["code"] == 0 for e in elements}
    sink = ["autovideosink", "sync=true"] if args.preview else ["fakesink", "sync=false"]
    pipeline = [launch, "-q", "videotestsrc", "num-buffers=90", "!", "video/x-raw,width=1280,height=720,framerate=30/1",
                "!", "videoconvert", "!", "vp8enc", "deadline=1", "!", "vp8dec", "!", "videoconvert", "!"] + sink
    start = time.monotonic()
    report["pipeline"] = run(pipeline, 45)
    report["pipeline"]["wall_seconds"] = round(time.monotonic() - start, 3)
    passed = report["pipeline"]["code"] == 0
    report["status"] = "synthetic_pass" if passed else "synthetic_failed"
    report["webrtc_elements_present"] = all(report["elements"][e] for e in ("webrtcbin", "rtpvp8pay", "rtpvp8depay", "nicesrc", "dtlssrtpenc"))
    print(json.dumps(report, indent=2))
    # Availability of plugins is not evidence of working WebRTC.
    return 0 if passed and report["webrtc_elements_present"] else 1

if __name__ == "__main__":
    sys.exit(main())
