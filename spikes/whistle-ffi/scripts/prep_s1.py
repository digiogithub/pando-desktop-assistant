"""S1 data prep: builds 16 kHz mono WAV sets + manifests under $SPIKE/sets/.

Sets (all refs in refs_<set>.tsv, wav lists in list_<set>.tsv = id<TAB>path):
  fleurs   200 random clips of FLEURS es_419 test (CC-BY-4.0, google/fleurs)
  mls      200 random clips of MLS Spanish test (CC-BY-4.0, facebook/multilingual_librispeech)
  cmd      50 own commands with project names x 3 Piper voices (synthetic, CC0/CC-BY/Unlicense)
  cmdnoisy the cmd set mixed with pink noise at 10 dB SNR
  lat3/lat10  30 FLEURS clips cut to exactly 3 s / 10 s (latency only)
Usage: venv/bin/python -I prep_s1.py <spike_cache_dir>
"""
import csv, io, os, random, subprocess, sys
import numpy as np, soundfile as sf

ROOT = sys.argv[1]
SETS = os.path.join(ROOT, "sets")
rng = random.Random(42)

def write(path, pcm, sr=16000):
    sf.write(path, np.clip(pcm, -1, 1).astype(np.float32), sr, subtype="PCM_16")

def resample(pcm, sr):
    if sr == 16000:
        return pcm.astype(np.float32)
    p = subprocess.run(["ffmpeg", "-v", "error", "-f", "f32le", "-ar", str(sr), "-ac", "1", "-i", "-",
                        "-f", "f32le", "-ar", "16000", "-ac", "1", "-"], input=pcm.astype(np.float32).tobytes(),
                       capture_output=True, check=True)
    return np.frombuffer(p.stdout, dtype=np.float32)

def out_set(name, rows):
    d = os.path.join(SETS, name); os.makedirs(d, exist_ok=True)
    with open(os.path.join(SETS, f"list_{name}.tsv"), "w") as fl, open(os.path.join(SETS, f"refs_{name}.tsv"), "w") as fr:
        for rid, pcm, ref in rows:
            p = os.path.join(d, f"{rid}.wav"); write(p, pcm)
            fl.write(f"{rid}\t{p}\n"); fr.write(f"{rid}\t{ref}\n")

def fleurs():
    base = os.path.join(ROOT, "data/fleurs")
    rows = list(csv.reader(open(os.path.join(base, "test.tsv")), delimiter="\t", quoting=csv.QUOTE_NONE))
    rng.shuffle(rows)
    sel, lat3, lat10 = [], [], []
    for r in rows[:200]:
        pcm, sr = sf.read(os.path.join(base, "audio/test", r[1]), dtype="float32")
        sel.append((r[1][:-4], resample(pcm, sr), r[2]))
    for r in rows[200:]:
        pcm, sr = sf.read(os.path.join(base, "audio/test", r[1]), dtype="float32"); pcm = resample(pcm, sr)
        if len(lat10) < 30 and len(pcm) >= 160000: lat10.append((r[1][:-4], pcm[:160000], "-"))
        elif len(lat3) < 30 and len(pcm) >= 48000: lat3.append((r[1][:-4], pcm[:48000], "-"))
    out_set("fleurs", sel); out_set("lat3", lat3); out_set("lat10", lat10)

def mls():
    import pyarrow.parquet as pq
    t = pq.read_table(os.path.join(ROOT, "data/mls/test.parquet")).to_pylist()
    rng.shuffle(t)
    rows = []
    for r in t[:200]:
        p = subprocess.run(["ffmpeg", "-v", "error", "-i", "-", "-f", "f32le", "-ar", "16000", "-ac", "1", "-"],
                           input=r["audio"]["bytes"], capture_output=True, check=True)
        rows.append((r["id"], np.frombuffer(p.stdout, dtype=np.float32), r["transcript"]))
    out_set("mls", rows)

def commands():
    import sherpa_onnx
    cmds = [l.strip() for l in open(os.path.join(os.path.dirname(__file__), "commands_es.txt")) if l.strip()]
    voices = [("davefx", "vits-piper-es_ES-davefx-medium/es_ES-davefx-medium.onnx"),
              ("sharvard", "vits-piper-es_ES-sharvard-medium/es_ES-sharvard-medium.onnx"),
              ("ald", "vits-piper-es_MX-ald-medium/es_MX-ald-medium.onnx")]
    clean, noisy = [], []
    nrng = np.random.default_rng(7)
    for vname, model in voices:
        mdir = os.path.join(ROOT, "models", os.path.dirname(model))
        cfg = sherpa_onnx.OfflineTtsConfig(model=sherpa_onnx.OfflineTtsModelConfig(
            vits=sherpa_onnx.OfflineTtsVitsModelConfig(model=os.path.join(ROOT, "models", model),
                tokens=os.path.join(mdir, "tokens.txt"), data_dir=os.path.join(mdir, "espeak-ng-data")), num_threads=4))
        tts = sherpa_onnx.OfflineTts(cfg)
        for i, c in enumerate(cmds):
            a = tts.generate(c, sid=0, speed=1.0)
            pcm = resample(np.array(a.samples, dtype=np.float32), a.sample_rate)
            pcm = np.concatenate([np.zeros(4000, np.float32), pcm, np.zeros(4000, np.float32)])
            rid = f"{vname}_{i:02d}"
            clean.append((rid, pcm, c))
            # pink-ish noise (1/f) at 10 dB SNR
            white = nrng.standard_normal(len(pcm) + 1)
            spec = np.fft.rfft(white); f = np.arange(len(spec)); f[0] = 1
            pink = np.fft.irfft(spec / np.sqrt(f))[: len(pcm)]
            ps, pn = np.mean(pcm ** 2), np.mean(pink ** 2)
            noisy.append((rid, pcm + pink * np.sqrt(ps / (pn * 10 ** (10 / 10))), c))
    out_set("cmd", clean); out_set("cmdnoisy", noisy)

if __name__ == "__main__":
    for step in sys.argv[2:] or ["fleurs", "mls", "commands"]:
        globals()[step](); print("done", step)
