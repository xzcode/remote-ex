"""Stage a new Info.plist for an independently reviewed HTTPS origin."""
import argparse
import plistlib
import re
from pathlib import Path
from urllib.parse import urlsplit

SOURCE = Path(__file__).resolve().parents[1] / "apps/mobile/ios/Info.plist"
def origin_host(value: str) -> str:
    if not isinstance(value, str) or len(value) > 512 or not re.fullmatch(r"https://[a-z0-9.-]+", value):
        raise ValueError("Use exact canonical HTTPS DNS origin, without path, port or credentials")
    parsed = urlsplit(value)
    host = parsed.hostname
    if not host or "." not in host or any(not re.fullmatch(r"[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?", label) for label in host.split(".")):
        raise ValueError("Invalid reviewed DNS host")
    if all(label.isdecimal() for label in host.split(".")):
        raise ValueError("Use DNS hostname, not numeric IP")
    return host

def stage(output: Path, origin: str) -> None:
    host = origin_host(origin)
    data = plistlib.loads(SOURCE.read_bytes())
    data["RemoteEXControllerOrigin"] = origin
    data["WKAppBoundDomains"] = [host]
    with output.open("xb") as stream:
        plistlib.dump(data, stream)

if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("output", type=Path)
    p.add_argument("origin")
    args = p.parse_args()
    stage(args.output, args.origin)
    print("Staged reviewed iOS origin plist; no service or signing configured.")
