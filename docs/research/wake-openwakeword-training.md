# Training a custom "Oye Pando" openWakeWord model (PDA-T-0142)

Status: **recipe only; not trained.** Per PDA-T-0142 this is needed only if the evaluation
(`docs/wake-eval/README.md`) misses the R3 targets with VAD + Whistle (today: FRR at 2 m is 14 % on the
synthetic set vs the 10 % target). Pretrained upstream openWakeWord models are CC BY-NC-SA and cannot be used,
so a model has to be trained from our own data.

## Where it plugs in

`pando_stt::wake::WakeDetector` is the seam: `check(segment, stt) -> Option<WakeHit>`. A trained model becomes
a first stage in front of the Whistle verification (gate -> verify), or replaces it. `HandsFree` needs no change.

## Toolkit

- Training (Python, development machine only): `livekit-wakeword[train,eval,export]` (PyTorch). Pipeline: synthetic
  positives -> augmentation (RIR, noise, gain) -> train classifier head on frozen openWakeWord mel/embedding models ->
  export ONNX.
- Inference in the worker: pure-Rust `livekit-wakeword` 0.1.x (ONNX via its own runtime) or `oww-rs` 0.3.x (tract).
  Use tract/Rust inference so no second ONNX Runtime enters `pando-stt` (ADR 0005/0007). Model ~200 KB plus the
  shared mel/embedding models (~2.5 MB).

## Recipe

1. Positives: "Oye Pando" (+ "Hey Pando") synthesised with many Spanish voices: Piper (`es_ES-davefx`,
   `es_ES-sharvard`, `es_MX-ald`, ...) and Kokoro `ef_dora` / `em_alex`, speeds 0.8-1.25, plus **real recordings**
   from >= 5 speakers (the same set as the R3 fixtures, kept apart from training data).
2. Negatives: >= 10 h of Spanish speech (Common Voice es, MLS es, podcasts), office/TV noise, the confusers
   ("oye panda", "oye Pablo", "pan de molde" ...), and the openWakeWord negative feature sets.
3. Augmentation: room impulse responses at 0.5 / 1 / 2 m, SNR 5-25 dB, laptop-mic EQ, +-6 dB gain.
4. Train with the target false-accept rate set to <= 0.2 / h on the validation negatives.
5. Export ONNX, convert/load in Rust, add `OwwDetector: WakeDetector` behind a cargo feature `oww`.
6. Evaluate with `wake-eval` on the same manifest; accept only if FAR and FRR both improve over VAD + Whistle.
7. Licence: the trained weights are ours (MIT); record training-data licences in NOTICE (MLS/Common Voice are CC-BY / CC0).

## Why it is not done yet

Needs the real recordings (R3 fixtures) to train on and, above all, to evaluate; synthetic far-field audio
alone would produce a model tuned to the simulator. Multi-hour GPU/CPU training is outside this batch.
