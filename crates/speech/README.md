# speech

Pure logic for voice output (no audio, no I/O, no network), shared by the app (MIT) and the `pando-tts` worker:

- `normalize`: markdown/code/ids/URLs/emoji/numbers/units/dates to speakable text, Spanish and English
  (PDA-T-0110, `PDA-SP-0007.R3`).
- `segment`: sentence splitter that respects abbreviations and numbers, with the first-chunk comma rule
  (PDA-T-0108, `PDA-SP-0007.R2`).
- `queue`: `SpeechQueue`, priority direct reply > conclusion > notification, waits while listening, mute, skip and
  barge-in (PDA-T-0111, `PDA-SP-0007.R4`).
