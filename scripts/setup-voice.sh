#!/usr/bin/env bash
# Download the default Piper voice (en_US-ryan-medium) used for cue synthesis.
#
# The voice is NOT committed to this repo. It is trained on the RyanSpeech corpus
# and licensed CC BY-NC 4.0 (non-commercial, attribution required). See
# docs/LICENSING.md before redistributing audio produced with it.
set -euo pipefail

VOICE="en_US-ryan-medium"
BASE="https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/ryan/medium"
DEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/voices"

mkdir -p "$DEST_DIR"

echo "Downloading ${VOICE} into ${DEST_DIR} ..."
for f in "${VOICE}.onnx" "${VOICE}.onnx.json"; do
  if [ -f "${DEST_DIR}/${f}" ]; then
    echo "  ✓ ${f} already present, skipping"
  else
    echo "  ↓ ${f}"
    curl -fL --retry 3 -o "${DEST_DIR}/${f}" "${BASE}/${f}?download=true"
  fi
done

echo
echo "Voice ready in ${DEST_DIR}."
echo
echo "Runtime dependencies (install separately if missing):"
echo "  • piper      — the TTS binary (https://github.com/rhasspy/piper/releases)"
echo "  • espeak-ng  — phonemizer Piper depends on"
echo "                 macOS:  brew install espeak-ng"
echo "                 Debian: sudo apt-get install espeak-ng"
echo
echo "Reminder: ${VOICE} is CC BY-NC 4.0 (non-commercial). See docs/LICENSING.md."
