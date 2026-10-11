"""S5 evaluation. Usage: venv/bin/python -I s5_eval.py <spike_cache_dir> <step> [...]

steps:
  vad <wav|list.tsv> <tag>      silero VAD over a long WAV (or each clip of a list); writes the
                                first 2.0 s of every speech segment to wake/seg_<tag>/ + list,
                                prints VAD CPU seconds / audio seconds.
  kws <wav|list.tsv> <thr> <score>  sherpa-onnx KWS (English gigaspeech zipformer, BPE tokens for
                                "oye pando"/"hey pando"); prints detections and CPU.
  scoreA <bench.jsonl>          applies the transcript match rules to Whistle results.
"""
import json, os, re, sys, time, unicodedata
import numpy as np, soundfile as sf, sherpa_onnx

ROOT = sys.argv[1]; W = os.path.join(ROOT, "wake"); M = os.path.join(ROOT, "models")
CHUNK = 1600  # 100 ms

def inputs(src):
    if src.endswith(".tsv"):
        for l in open(src):
            cid, p, *_ = l.rstrip("\n").split("\t"); yield cid, p
    else:
        yield os.path.basename(src)[:-4], src

def read(p):
    a, sr = sf.read(p, dtype="float32"); assert sr == 16000; return a

def vad(src, tag):
    cfg = sherpa_onnx.VadModelConfig(); cfg.silero_vad.model = f"{M}/silero_vad.onnx"
    cfg.silero_vad.min_silence_duration = 0.3; cfg.silero_vad.min_speech_duration = 0.25
    cfg.silero_vad.threshold = 0.5; cfg.sample_rate = 16000; cfg.num_threads = 1
    od = os.path.join(W, f"seg_{tag}"); os.makedirs(od, exist_ok=True)
    cpu = audio = 0.0; n = 0
    with open(os.path.join(W, f"list_seg_{tag}.tsv"), "w") as fl:
        for cid, p in inputs(src):
            a = read(p); audio += len(a) / 16000
            v = sherpa_onnx.VoiceActivityDetector(cfg, buffer_size_in_seconds=60)
            segs = []
            t = time.process_time()
            for i in range(0, len(a), CHUNK):
                v.accept_waveform(a[i:i + CHUNK])
                while not v.empty():
                    segs.append((v.front.start, np.array(v.front.samples[:32000], np.float32))); v.pop()
            v.flush()
            while not v.empty():
                segs.append((v.front.start, np.array(v.front.samples[:32000], np.float32))); v.pop()
            cpu += time.process_time() - t
            for k, (start, s) in enumerate(segs):
                sp = os.path.join(od, f"{cid}_{k:05d}.wav"); sf.write(sp, s, 16000, subtype="FLOAT")
                fl.write(f"{cid}|{start/16000:.2f}\t{sp}\n"); n += 1
    print(json.dumps({"step": "vad", "tag": tag, "segments": n, "audio_s": round(audio, 1), "vad_cpu_s": round(cpu, 2),
                      "vad_core_pct": round(100 * cpu / audio, 3)}))

def kws(src, thr, score):
    d = f"{M}/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01"
    kw = os.path.join(W, "kws_keywords.txt")
    with open(kw, "w") as f:
        f.write("▁O Y E ▁PA ND O @OYE_PANDO\n▁O Y ▁PA ND O @OY_PANDO\n▁HE Y ▁PA ND O @HEY_PANDO\n")
    k = sherpa_onnx.KeywordSpotter(tokens=f"{d}/tokens.txt", encoder=f"{d}/encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
        decoder=f"{d}/decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx", joiner=f"{d}/joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx",
        num_threads=1, keywords_file=kw, keywords_score=float(score), keywords_threshold=float(thr), max_active_paths=4)
    cpu = audio = 0.0; hits = {}; ndet = 0; nclips = 0
    for cid, p in inputs(src):
        a = read(p); audio += len(a) / 16000; nclips += 1
        s = k.create_stream(); t = time.process_time(); det = []
        for i in range(0, len(a), CHUNK):
            s.accept_waveform(16000, a[i:i + CHUNK])
            while k.is_ready(s):
                k.decode_stream(s); r = k.get_result(s)
                if r: det.append((round(i / 16000, 1), r)); k.reset_stream(s)
        s.input_finished()
        while k.is_ready(s):
            k.decode_stream(s); r = k.get_result(s)
            if r: det.append((round(len(a) / 16000, 1), r)); k.reset_stream(s)
        cpu += time.process_time() - t
        if det: hits[cid] = det; ndet += len(det)
    print(json.dumps({"step": "kws", "src": os.path.basename(src), "thr": thr, "score": score, "clips": nclips,
                      "clips_detected": len(hits), "detections": ndet, "audio_s": round(audio, 1),
                      "cpu_s": round(cpu, 2), "core_pct": round(100 * cpu / audio, 3),
                      "sample": list(hits.items())[:5]}, ensure_ascii=False))

def norm(s):
    s = re.sub(r"[^\w\s]", " ", s.lower())
    return "".join(c for c in unicodedata.normalize("NFD", s) if unicodedata.category(c) != "Mn").split()

def lev(a, b):
    d = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        p, d[0] = d[0], i
        for j, cb in enumerate(b, 1):
            p, d[j] = d[j], min(d[j] + 1, d[j - 1] + 1, p + (ca != cb))
    return d[-1]

def match(words, rule):
    head = words[:3]
    if rule == "strict":  # "pando" as one of the first two words
        return "pando" in words[:2]
    # fuzzy: any 1-2 word window in the first 3 words close to "oye pando"/"hey pando"/"pando"
    for i in range(len(head)):
        for j in (1, 2):
            w = " ".join(head[i:i + j])
            for target in ("oye pando", "hey pando", "pando"):
                if len(w) >= 4 and 1 - lev(w, target) / max(len(w), len(target)) >= 0.8:
                    return True
    return False

def scoreA(path):
    rows = [json.loads(l) for l in open(path) if l.startswith("{")]
    clips = {}
    for r in rows:
        cid = r["id"].split("|")[0]; w = norm(r["result"].get("text", ""))
        c = clips.setdefault(cid, {"strict": 0, "fuzzy": 0, "texts": []})
        for rule in ("strict", "fuzzy"):
            if match(w, rule): c[rule] += 1; c["texts"].append(r["result"]["text"])
    out = {"step": "scoreA", "file": os.path.basename(path), "segments": len(rows), "whistle_ms_total": round(sum(r["ms"] for r in rows)),
           "clips_with_segments": len(clips)}
    for rule in ("strict", "fuzzy"):
        out[f"{rule}_clips_detected"] = sum(1 for c in clips.values() if c[rule]); out[f"{rule}_detections"] = sum(c[rule] for c in clips.values())
    out["sample_fuzzy"] = [t for c in clips.values() for t in c["texts"]][:8]
    print(json.dumps(out, ensure_ascii=False))

if __name__ == "__main__":
    {"vad": vad, "kws": kws, "scoreA": scoreA}[sys.argv[2]](*sys.argv[3:])
