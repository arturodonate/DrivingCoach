# Licensing

This project has **two independent licensing concerns**: the project's own code,
and the Piper voice model it downloads at install time. They are kept separate on
purpose so the voice's non-commercial terms are never conflated with the code.

## 1. Project code — MIT

All source code in this repository is licensed under the [MIT License](../LICENSE).
You may use, modify, and redistribute the code (including commercially) under MIT.

## 2. Voice model — depends on the voice you install

The default cue voice is **`en_US-ryan-medium`** (US English, male, `medium`
quality tier). It is **not committed to this repository**; the setup script
downloads it from the official [`rhasspy/piper-voices`](https://huggingface.co/rhasspy/piper-voices)
Hugging Face repository (`en/en_US/ryan/medium/`). This keeps redistribution of
*this repo* clean regardless of the voice license.

### `en_US-ryan-medium`: CC BY-NC 4.0 (non-commercial)

- The **Piper engine** itself is MIT.
- The **voice** is trained on the **RyanSpeech** corpus, which is licensed
  **[CC BY-NC 4.0](https://creativecommons.org/licenses/by-nc/4.0/)** —
  *non-commercial use, attribution required*.
- The voice **inherits the dataset license**, so **using this project as
  configured (with `en_US-ryan-medium`) is for non-commercial use only.**

**Required attribution** when distributing audio produced by this voice:

> The `en_US-ryan-medium` voice is derived from the RyanSpeech corpus.
> Zandie, R., Mahoor, M. H., Madsen, J., & Emamian, E. S. (2021).
> *RyanSpeech: A Corpus for Conversational Text-to-Speech Synthesis.*
> Licensed under CC BY-NC 4.0.

### Want commercial use? Swap the voice (config-only).

Nothing structural depends on the voice — cues are a closed vocabulary cached to
WAV, so changing voices means clearing the WAV cache and re-synthesizing (spec
§8.2). Point the configured voice path at a permissively licensed Piper voice:

- **LibriTTS / LibriTTS-R-derived voices: CC BY 4.0** — attribution only,
  commercial use OK.
- **LJSpeech-derived voices:** effectively public domain.

Always audit the target voice's own `MODEL_CARD` before relying on it — Piper
voices vary in their licensing.

## Summary

| Component | License | Commercial use? |
|---|---|---|
| This project's code | MIT | ✅ Yes |
| `en_US-ryan-medium` voice (default) | CC BY-NC 4.0 | ❌ No (non-commercial) |
| LibriTTS-R-derived voice (swap-in) | CC BY 4.0 | ✅ Yes (with attribution) |
