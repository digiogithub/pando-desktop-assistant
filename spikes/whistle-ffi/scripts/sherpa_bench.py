"""Baseline STT through sherpa-onnx (same JSONL as the Rust `bench`).
Usage: venv/bin/python -I sherpa_bench.py <spike_cache_dir> <parakeet|whisper-small> <list.tsv> [threads]"""
import json, os, sys, time
import soundfile as sf, sherpa_onnx

root, model, lst = sys.argv[1:4]
threads = int(sys.argv[4]) if len(sys.argv) > 4 else 4
m = os.path.join(root, "models")
if model == "parakeet":
    d = os.path.join(m, "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8")
    rec = sherpa_onnx.OfflineRecognizer.from_transducer(encoder=f"{d}/encoder.int8.onnx", decoder=f"{d}/decoder.int8.onnx",
        joiner=f"{d}/joiner.int8.onnx", tokens=f"{d}/tokens.txt", num_threads=threads, model_type="nemo_transducer")
else:
    d = os.path.join(m, "sherpa-onnx-whisper-small")
    rec = sherpa_onnx.OfflineRecognizer.from_whisper(encoder=f"{d}/small-encoder.int8.onnx", decoder=f"{d}/small-decoder.int8.onnx",
        tokens=f"{d}/small-tokens.txt", language="es", task="transcribe", num_threads=threads)
for line in open(lst):
    rid, path = line.rstrip("\n").split("\t")
    pcm, sr = sf.read(path, dtype="float32")
    t = time.perf_counter()
    s = rec.create_stream(); s.accept_waveform(sr, pcm); rec.decode_stream(s)
    ms = (time.perf_counter() - t) * 1e3
    print(json.dumps({"id": rid, "ms": ms, "audio_s": len(pcm) / sr, "result": {"text": s.result.text.strip()}}, ensure_ascii=False), flush=True)
