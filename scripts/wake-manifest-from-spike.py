#!/usr/bin/env python3
"""Build a wake-eval manifest (JSON) from the spike S5 dataset in ~/.cache/pando-assistant-spikes/wake.

The dataset itself is NEVER committed; only this manifest format and generator are. Generate the
audio with spikes/wake-word/prep_s5.py (see docs/spikes/S5-wake-word.md) or record real clips and
write a manifest of the same shape by hand.

usage: scripts/wake-manifest-from-spike.py [spike_cache_dir] > manifest.json
"""
import json, os, re, sys

root = os.path.join(sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/.cache/pando-assistant-spikes"), "wake")

def rows(name):
    p = os.path.join(root, name)
    if not os.path.exists(p):
        return
    for line in open(p, encoding="utf-8"):
        cid, path, *rest = line.rstrip("\n").split("\t")
        yield cid, path, (rest[0] if rest else "")

def distance(cid):
    m = re.search(r"_(\d+(?:\.\d+)?)m$", cid)
    return float(m.group(1)) if m else None

def klass(phrase):
    p = phrase.lower().lstrip("¿¡ ")
    return "oye" if p.startswith("oye") else "hey" if p.startswith("hey") else "bare"

def voice(cid):
    return cid.split("_")[0]

manifest = {
    "version": 1,
    "note": "audio is generated/recorded locally and never committed",
    "negatives": [],
    "idle": [],
    "positives": [{"id": c, "path": p, "phrase": ph, "class": klass(ph), "distance_m": distance(c), "voice": voice(c)} for c, p, ph in rows("list_pos.tsv")],
    "confusers": [{"id": c, "path": p, "phrase": ph} for c, p, ph in rows("list_conf.tsv")],
}
for name, key in (("negatives.wav", "negatives"), ("idle_office_10min.wav", "idle")):
    p = os.path.join(root, name)
    if os.path.exists(p):
        manifest[key].append({"id": name[:-4], "path": p})
json.dump(manifest, sys.stdout, ensure_ascii=False, indent=1)
print()
