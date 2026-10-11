"""S5 data prep. Usage: venv/bin/python -I prep_s5.py <spike_cache_dir> [neg_hours]

negatives: neg_hours (default 3) of continuous Spanish speech = MLS Spanish test audiobook clips
  (CC-BY-4.0) joined with 0.3-1.5 s gaps, mixed with synthetic office noise (pink noise +
  keyboard-like clicks) at 15 dB SNR. Written as 16 kHz mono float WAV, ~690 MB/3 h.
positives: "Oye Pando" (+ an optional command) synthesised with 3 Piper voices x 3 speeds x
  3 simulated distances (0.5 m / 1 m / 2 m = attenuation + synthetic exponential reverb
  + noise at 25/15/8 dB SNR), plus "Hey Pando"/"Pando" variants -> clips with leading silence.
confusers: near-miss phrases ("oye panda", "pan dulce", "mando", "oye Pablo"...) as extra negatives.
"""
import os, random, subprocess, sys
import numpy as np, soundfile as sf

ROOT = sys.argv[1]; HOURS = float(sys.argv[2]) if len(sys.argv) > 2 else 3.0
OUT = os.path.join(ROOT, "wake"); os.makedirs(OUT, exist_ok=True)
rng = np.random.default_rng(11); prng = random.Random(11)

def pink(n):
    spec = np.fft.rfft(rng.standard_normal(n + 1)); f = np.arange(len(spec)); f[0] = 1
    x = np.fft.irfft(spec / np.sqrt(f))[:n]; return (x / (np.std(x) + 1e-9)).astype(np.float32)

def office(n):
    x = pink(n) * 0.5
    for _ in range(n // 16000 * 3):  # ~3 key clicks per second
        i = rng.integers(0, max(n - 400, 1)); x[i:i + 400] += (rng.standard_normal(400) * np.exp(-np.arange(400) / 60)).astype(np.float32) * 2
    return x

def mix(sig, noise, snr_db):
    ps = np.mean(sig ** 2) + 1e-12; pn = np.mean(noise ** 2) + 1e-12
    return sig + noise * np.sqrt(ps / (pn * 10 ** (snr_db / 10)))

def reverb(sig, rt60):
    n = int(16000 * rt60); t = np.arange(n) / 16000
    ir = rng.standard_normal(n) * np.exp(-6.9 * t / rt60); ir[0] = 1.0; ir /= np.sqrt(np.sum(ir ** 2))
    return np.convolve(sig, ir.astype(np.float32))[: len(sig) + n // 2].astype(np.float32)

def to16k(pcm, sr):
    p = subprocess.run(["ffmpeg", "-v", "error", "-f", "f32le", "-ar", str(sr), "-ac", "1", "-i", "-", "-f", "f32le", "-ar", "16000", "-"],
                       input=pcm.astype(np.float32).tobytes(), capture_output=True, check=True)
    return np.frombuffer(p.stdout, dtype=np.float32)

def negatives():
    import pyarrow.parquet as pq
    rows = pq.read_table(os.path.join(ROOT, "data/mls/test.parquet")).to_pylist(); prng.shuffle(rows)
    parts, total = [], 0
    for r in rows:
        p = subprocess.run(["ffmpeg", "-v", "error", "-i", "-", "-f", "f32le", "-ar", "16000", "-ac", "1", "-"],
                           input=r["audio"]["bytes"], capture_output=True, check=True)
        a = np.frombuffer(p.stdout, dtype=np.float32)
        gap = np.zeros(int(16000 * prng.uniform(0.3, 1.5)), np.float32)
        parts += [a, gap]; total += len(a) + len(gap)
        if total >= HOURS * 3600 * 16000: break
    s = np.concatenate(parts)
    s = mix(s, office(len(s)), 15)
    sf.write(os.path.join(OUT, "negatives.wav"), (s / max(1.0, np.max(np.abs(s)))).astype(np.float32), 16000, subtype="FLOAT")
    print("negatives hours", len(s) / 16000 / 3600)

def tts_all(phrases):
    import sherpa_onnx
    voices = [("davefx", "vits-piper-es_ES-davefx-medium", "es_ES-davefx-medium.onnx"),
              ("sharvard", "vits-piper-es_ES-sharvard-medium", "es_ES-sharvard-medium.onnx"),
              ("ald", "vits-piper-es_MX-ald-medium", "es_MX-ald-medium.onnx")]
    out = []
    for vname, d, f in voices:
        md = os.path.join(ROOT, "models", d)
        tts = sherpa_onnx.OfflineTts(sherpa_onnx.OfflineTtsConfig(model=sherpa_onnx.OfflineTtsModelConfig(
            vits=sherpa_onnx.OfflineTtsVitsModelConfig(model=f"{md}/{f}", tokens=f"{md}/tokens.txt", data_dir=f"{md}/espeak-ng-data"), num_threads=4)))
        for pi, text in enumerate(phrases):
            for speed in (0.85, 1.0, 1.2):
                a = tts.generate(text, sid=0, speed=speed)
                out.append((f"{vname}_{pi:02d}_s{speed}", text, to16k(np.array(a.samples), a.sample_rate)))
    return out

def write_set(name, clips, distances):
    d = os.path.join(OUT, name); os.makedirs(d, exist_ok=True)
    with open(os.path.join(OUT, f"list_{name}.tsv"), "w") as fl:
        for cid, text, pcm in clips:
            for dist, (gain, rt60, snr) in distances.items():
                x = reverb(pcm * gain, rt60)
                x = np.concatenate([np.zeros(16000, np.float32), x, np.zeros(8000, np.float32)])
                x = mix(x, office(len(x)), snr)
                p = os.path.join(d, f"{cid}_{dist}.wav"); sf.write(p, (x / max(1.0, np.max(np.abs(x)))).astype(np.float32), 16000, subtype="FLOAT")
                fl.write(f"{cid}_{dist}\t{p}\t{text}\n")

DIST = {"0.5m": (1.0, 0.25, 25), "1m": (0.5, 0.4, 15), "2m": (0.25, 0.6, 8)}

def positives():
    phrases = ["Oye Pando.", "Oye Pando, ¿qué tareas hay pendientes?", "Oye Pando, cancela la migración.",
               "Hey Pando, abre Bitácora.", "Oye, Pando.", "Pando, para."]
    write_set("pos", tts_all(phrases), DIST)

def confusers():
    phrases = ["Oye, panda.", "Oye Pablo, ven.", "Me comí un pan dulce.", "Pásame el mando.", "Oye, ¿y tu banda?",
               "El panda come bambú.", "Hoy pagando la cuenta.", "Oye, cuándo vienes.", "Fernando, para ya.", "Pan de molde."]
    write_set("conf", tts_all(phrases), {"1m": DIST["1m"]})

if __name__ == "__main__":
    for step in sys.argv[3:] or ["positives", "confusers", "negatives"]:
        globals()[step](); print("done", step)
