#!/usr/bin/env python3
"""Parse one actual Android df -k /data result; reject ambiguous rows."""
import re
import sys

def available(text):
    if len(text) > 8192:
        raise ValueError("Oversized df output")
    rows = []
    for line in text.splitlines():
        fields = line.split()
        if not fields or fields[0] == "Filesystem":
            continue
        if len(fields) != 6:
            raise ValueError("Unexpected df row")
        device, total, used, free, percent, mount = fields
        if not re.fullmatch(r"/dev/block/[A-Za-z0-9_./-]+", device):
            raise ValueError("Unexpected internal storage device")
        if not all(re.fullmatch(r"[0-9]{1,15}", v) for v in (total, used, free)) or not re.fullmatch(r"[0-9]{1,3}%", percent):
            raise ValueError("Unexpected df capacity")
        if not re.fullmatch(r"/data(?:/user/0)?", mount):
            raise ValueError("Unexpected internal storage mount")
        t, u, a = map(int, (total, used, free))
        if not (0 <= a <= t and 0 <= u <= t and u+a <= t and t > 0 and int(percent[:-1]) <= 100):
            raise ValueError("Invalid df totals")
        rows.append(a)
    if len(rows) != 1:
        raise ValueError("Expected exactly one internal-storage row")
    return rows[0]

def self_test():
    header = "Filesystem 1K-blocks Used Available Use% Mounted on\n"
    assert available(header + "/dev/block/dm-6 6082144 757508 5182424 13% /data/user/0\n") == 5182424
    assert available(header + "/dev/block/dm-55 6082144 1193592 4746340 21% /data/user/0\n") == 4746340
    assert available(header + "/dev/block/dm-0 8000000 1000000 6000000 15% /data\r\n") == 6000000
    assert available(header + "/dev/block/dm-0 1000 1000 0 100% /data\n") == 0
    for bad in ("", header, header + "tmpfs 100 10 90 10% /data\n", header + "/dev/block/dm-1 1000 10 990 1% /storage\n", header + "/dev/block/dm-1 1000 10 9999 1% /data\n", header + "/dev/block/dm-1 1000 10 990 1% /data\n/dev/block/dm-2 1000 10 990 1% /data/user/0\n"):
        try:
            available(bad)
        except ValueError:
            continue
        raise AssertionError("Accepted malformed or ambiguous capacity")
    print("PASS Android storage parser: both observed /data/user/0 rows, /data, full disk and malformed/ambiguous rejection")

if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    elif not sys.argv[1:]:
        try:
            print(available(sys.stdin.read(8193)))
        except ValueError as e:
            print(str(e), file=sys.stderr)
            sys.exit(1)
    else:
        sys.exit("Unsupported argument")
