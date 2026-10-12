# Wake-word evaluation (PDA-T-0141, PDA-SP-0011.R3)

`wake-eval` (crate `pando-stt`, binary `wake-eval`) runs the **production hands-free pipeline** offline:
energy or Silero VAD -> Whistle verification with the `Pando / Oye Pando / Hey Pando` bias -> fuzzy match.
It reports false accepts per hour, FRR per phrase class and distance, confuser hits and CPU (% of one core).

No audio is committed. The dataset is generated or recorded locally and described by a manifest.

## Manifest (JSON)

```json
{
  "version": 1,
  "negatives": [{"id": "office3h", "path": "/data/negatives.wav"}],
  "idle":      [{"id": "quiet10", "path": "/data/idle.wav"}],
  "positives": [{"id": "ana_2m_01", "path": "/data/pos/ana_2m_01.wav", "phrase": "Oye Pando.",
                 "class": "oye", "distance_m": 2.0, "voice": "ana"}],
  "confusers": [{"id": "panda_01", "path": "/data/conf/panda_01.wav", "phrase": "Oye, panda."}]
}
```

All WAVs are 16 kHz mono (int16 or f32). `negatives` must contain no wake phrase (office, podcast, TV);
R3 asks for 3 h. `positives` need >= 5 real speakers at 0.5 / 1 / 2 m on the reference laptop microphone.
`class` is `oye`, `hey` or `bare`; the R3 FRR target applies to `oye`.

## Run

```bash
scripts/fetch-needle.sh --model                         # engine + whistle.cact
# synthetic dataset (spike S5, see docs/spikes/S5-wake-word.md) or your own recordings:
scripts/wake-manifest-from-spike.py > /tmp/manifest.json
cargo build --release -p pando-stt --bin wake-eval
target/release/wake-eval --manifest /tmp/manifest.json --model vendor/needle/models/whistle.cact \
    [--vad silero --vad-model silero_vad.onnx]  # silero needs --features silero-vad and a TTS-off sherpa build
    [--sensitivity 0.4] [--subset 40] [--out report.json] [--check]
```

`--subset N` evaluates N evenly spaced entries per list (nightly runs); `--check` exits 1 when a target is
missed (FAR < 1/h, FRR@2 m < 10 %, idle CPU < 3 % of a core).

## Results (synthetic S5 dataset: 3.0 h negatives, 162 positives, 90 confusers; linux-x86_64, Core Ultra 9 285)

| Pipeline | FAR / h | FRR "Oye Pando" 0.5 / 1 / 2 m | confusers fired | CPU, 3 h dense speech | CPU, 10 min quiet |
|---|---|---|---|---|---|
| energy VAD (default), sensitivity 0.4 | **0.33** | 6 % / 3 % / **14 %** | 13 / 90 | 3.0 % | 0.008 % |
| energy VAD, sensitivity 0.7 | 0.33 | 6 % / 3 % / 14 % | 14 / 90 | 2.7 % | 0.007 % |
| Silero v5 (sherpa) | 1.00 | 8 % / 22 % / 14 % | 13 / 90 | 3.9 % | 0.18 % |

Targets (PDA-SP-0011.R3): FAR < 1 / h (met by the energy VAD), FRR at 2 m < 10 % (**not met: 14 %**), idle CPU < 3 % (met;
the audiobook stream is a worst case of continuous speech, so dense-speech CPU is about the 3 % budget).
Sensitivity does not move the 2 m FRR: the misses are Whistle transcription failures at 8 dB SNR, not fuzzy-match
rejections. The dataset is synthetic (Piper voices, simulated distance), so this is a lower bound and **R3 is not signed
off** until real recordings exist; if 2 m stays above 10 % on real data, train the first-stage model
(`docs/research/wake-openwakeword-training.md`, PDA-T-0142).
