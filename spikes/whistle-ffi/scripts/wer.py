"""WER + latency summary. Usage: python -I wer.py <refs.tsv> <hyp.jsonl> [--keywords file]
Normalisation: lowercase, strip punctuation (keep accents/ñ), collapse spaces; hyphens -> space.
Also reports accent-insensitive WER and, with --keywords, keyword recall (exact term hits)."""
import json, re, sys, unicodedata
import numpy as np, jiwer

def norm(s, accents=True):
    s = s.lower().replace("-", " ")
    s = re.sub(r"[^\w\s]", " ", s)
    if not accents:
        s = "".join(c for c in unicodedata.normalize("NFD", s) if unicodedata.category(c) != "Mn")
    return " ".join(s.split())

refs = dict(l.rstrip("\n").split("\t", 1) for l in open(sys.argv[1]))
hyps = [json.loads(l) for l in open(sys.argv[2]) if l.startswith("{")]
R = [norm(refs[h["id"]]) for h in hyps]; H = [norm(h["result"].get("text", "")) for h in hyps]
Ra = [norm(refs[h["id"]], False) for h in hyps]; Ha = [norm(h["result"].get("text", ""), False) for h in hyps]
ms = np.array([h["ms"] for h in hyps]); au = np.array([h["audio_s"] for h in hyps])
out = {"n": len(hyps), "wer": round(100 * jiwer.wer(R, H), 2), "wer_noacc": round(100 * jiwer.wer(Ra, Ha), 2),
       "p50_ms": round(float(np.percentile(ms, 50)), 1), "p95_ms": round(float(np.percentile(ms, 95)), 1),
       "rtf": round(float(ms.sum() / 1e3 / au.sum()), 4)}
if "--keywords" in sys.argv:
    kws = [norm(k, False) for k in open(sys.argv[sys.argv.index("--keywords") + 1]) if k.strip()]
    tot = hit = 0
    for r, h in zip(Ra, Ha):
        for k in kws:
            if re.search(rf"\b{re.escape(k)}\b", r):
                tot += 1; hit += bool(re.search(rf"\b{re.escape(k)}\b", h))
    out["kw_recall"] = round(100 * hit / max(tot, 1), 1); out["kw_total"] = tot
print(json.dumps(out))
