# Windows packaging (PDA-T-0146)

Targets and workers (ADR 0004/0007):

| Binary | Target | Why |
|---|---|---|
| `pando-assistant.exe` | `x86_64-pc-windows-msvc` | GPUI |
| `pando-tts.exe` | `x86_64-pc-windows-msvc` | the prebuilt sherpa-onnx static libs are MSVC |
| `pando-stt.exe` | `x86_64-pc-windows-gnullvm` (llvm-mingw) | libneedle.a is GNU ABI |

Build: `cargo build --release -p app -p pando-tts`, then
`cargo build --release -p pando-stt --target x86_64-pc-windows-gnullvm` and copy the result next
to the others in `target\release\`. Then `cargo xtask bundle --no-build --formats nsis,wix`
(config: `packaging/Packager.toml`, `[nsis]` and `[wix]`), then `packaging/windows/sign.ps1`.
Not run yet: needs a Windows host and a code-signing certificate.
