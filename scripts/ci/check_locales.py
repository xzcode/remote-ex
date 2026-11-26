#!/usr/bin/env python3
"""Check all current source dictionary keys, without translation services."""
import json, pathlib, re
root = pathlib.Path("apps/desktop/src")
s = (root/"i18n.ts").read_text(encoding="utf-8")
en, zh = s.split("export const zh",1)
pattern = r'(?:^|[,\{]\s*|\n\s*)([A-Za-z]\w*)\s*:\s*("(?:\\.|[^"\\])*")'
def parse(text):
    pairs = [(k,json.loads(v)) for k,v in re.findall(pattern,text)]
    assert len(pairs)==len({k for k,_ in pairs}), "duplicate source key"
    assert all(v.strip() for _,v in pairs)
    return dict(pairs)
english,chinese = parse(en),parse(zh)
assert english.keys()==chinese.keys(), "English/Chinese dictionary mismatch"
rows = []
for line in (root/"locales.ts").read_text(encoding="utf-8").splitlines():
    if line.startswith('["'):
        rows.append(json.loads(line.removesuffix(",")))
assert len(rows)==len({r[0] for r in rows}), "duplicate translated key"
assert {r[0] for r in rows}==english.keys(), "translation key coverage mismatch"
assert all(len(r)==7 and all(isinstance(v,str) and v.strip() for v in r) for r in rows)
print(f"PASS {len(english)} keys in all eight language dictionaries (not linguistic or native-dialog acceptance)")
