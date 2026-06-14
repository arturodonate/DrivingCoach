# Download the default Piper voice (en_US-ryan-medium) used for cue synthesis.
#
# The voice is NOT committed to this repo. It is trained on the RyanSpeech corpus
# and licensed CC BY-NC 4.0 (non-commercial, attribution required). See
# docs/LICENSING.md before redistributing audio produced with it.
$ErrorActionPreference = "Stop"

$Voice = "en_US-ryan-medium"
$Base  = "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/ryan/medium"
$DestDir = Join-Path (Split-Path -Parent $PSScriptRoot) "voices"

New-Item -ItemType Directory -Force -Path $DestDir | Out-Null

Write-Host "Downloading $Voice into $DestDir ..."
foreach ($f in @("$Voice.onnx", "$Voice.onnx.json")) {
    $out = Join-Path $DestDir $f
    if (Test-Path $out) {
        Write-Host "  [skip] $f already present"
    } else {
        Write-Host "  [get]  $f"
        Invoke-WebRequest -Uri "$Base/$f`?download=true" -OutFile $out
    }
}

Write-Host ""
Write-Host "Voice ready in $DestDir."
Write-Host ""
Write-Host "Runtime dependencies (install separately if missing):"
Write-Host "  - piper      : the TTS binary (https://github.com/rhasspy/piper/releases)"
Write-Host "  - espeak-ng  : phonemizer Piper depends on"
Write-Host ""
Write-Host "Reminder: $Voice is CC BY-NC 4.0 (non-commercial). See docs/LICENSING.md."
