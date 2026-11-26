#!/usr/bin/env python3
"""Select an exact, reviewed SDK image from sdkmanager --list output.

Only Google APIs x86_64 phone candidates are accepted. Image existence is
not WebView compatibility; supported mode still requires full runtime proof.
"""
import sys

LEGACY = "system-images;android-36;google_apis;x86_64"
MODERN = (
    "system-images;android-37.2;google_apis_ps16k;x86_64",
    "system-images;android-37.2;google_apis;x86_64",
    "system-images;android-37.1;google_apis_ps16k;x86_64",
    "system-images;android-37.1;google_apis;x86_64",
    "system-images;android-37.0;google_apis_ps16k;x86_64",
    "system-images;android-37.0;google_apis;x86_64",
    "system-images;android-37;google_apis_ps16k;x86_64",
    "system-images;android-37;google_apis;x86_64",
    "system-images;android-36.1;google_apis;x86_64",
    "system-images;android-36.1;google_apis_ps16k;x86_64",
)

def select(mode, listing):
    if mode not in ("legacy", "supported") or len(listing) > 2_000_000:
        raise ValueError("Invalid image selection input")
    # SDK Manager prints fixed package identifiers before a pipe. Do not
    # execute or install values from arbitrary catalog descriptions.
    paths = {line.split("|", 1)[0].strip() for line in listing.splitlines() if "|" in line}
    candidates = (LEGACY,) if mode == "legacy" else MODERN
    for path in candidates:
        if path in paths:
            return path
    raise ValueError("No reviewed "+mode+" Google APIs x86_64 system image is present in the SDK catalog")

def self_test():
    assert select("legacy", LEGACY+" | 7 | image") == LEGACY
    assert select("supported", MODERN[-2]+" | 4 | image\n"+MODERN[0]+" | 1 | image") == MODERN[0]
    assert select("supported", MODERN[-2]+" | 4 | image") == MODERN[-2]
    for bad in (LEGACY+" | 7 | image", MODERN[0]+"-fake | 1 | image",
                "system-images;android-37.2;google_apis;arm64-v8a | 1 | image",
                "description | "+MODERN[0], ""):
        try:
            select("supported", bad)
        except ValueError:
            pass
        else:
            raise AssertionError("Unexpected image selected")
    print("PASS SDK catalog selection: exact stable candidates, deterministic priority and no legacy downgrade")

if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    elif len(sys.argv) == 2:
        try:
            print(select(sys.argv[1], sys.stdin.read(2_000_001)))
        except ValueError as error:
            print(str(error), file=sys.stderr)
            sys.exit(1)
    else:
        sys.exit("Usage: select-android-image.py legacy|supported (sdkmanager listing on stdin)")
