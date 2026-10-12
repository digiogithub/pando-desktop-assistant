# Sign the Windows installers and binaries (PDA-T-0146). Windows only.
# Secrets from the environment:
#   WIN_CERT_PFX_BASE64 + WIN_CERT_PASSWORD   (code-signing certificate), or
#   AZURE_TRUSTED_SIGNING_* with `signtool /dlib` (set WIN_SIGN_COMMAND to override).
# usage: powershell -File packaging/windows/sign.ps1 [-Out dist\packages]
param([string]$Out = "dist\packages")
$ErrorActionPreference = "Stop"
if (-not $env:WIN_CERT_PFX_BASE64) { throw "WIN_CERT_PFX_BASE64 is not set" }
$pfx = Join-Path $env:TEMP "pando-sign.pfx"
[IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WIN_CERT_PFX_BASE64))
try {
  $targets = Get-ChildItem -Path $Out -Include *.exe,*.msi -Recurse
  foreach ($f in $targets) {
    & signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 `
      /f $pfx /p $env:WIN_CERT_PASSWORD $f.FullName
    if ($LASTEXITCODE -ne 0) { throw "signtool failed for $($f.FullName)" }
    & signtool verify /pa $f.FullName
  }
} finally { Remove-Item $pfx -Force -ErrorAction SilentlyContinue }
# Note: pando-assistant.exe, pando-stt.exe and pando-tts.exe must be signed BEFORE packaging
# (sign target\release\*.exe, then run `cargo xtask bundle --no-build --formats nsis,wix`).
