# Typeless Research and Local-First Adaptation

Date: 2026-03-09

## Sources

- https://www.typeless.com/
- https://www.typeless.com/manifesto
- https://www.typeless.com/pricing
- https://www.typeless.com/help/troubleshooting/dictation-limit
- https://www.typeless.com/help/troubleshooting/missing-transcript
- https://typeies.com/

## What Typeless is selling

Typeless is not positioned as a raw speech-to-text tool. Its public site presents it as a cross-app writing layer where speech becomes polished writing in real time.

The main product pillars on the site are:

- Voice-first input: "Speak, don't type" and "4x faster than typing" are the core framing.
- Cross-app coverage: it emphasizes working across desktop and mobile, and "wherever you work."
- Auto-editing during dictation: filler-word removal, repetition cleanup, self-correction cleanup, and auto-formatting are central.
- Personalization: style and tone adaptation, a personal dictionary, and multilingual dictation are treated as core quality features rather than add-ons.
- Post-dictation intelligence: selected-text editing, summaries, explanations, translations, and quick actions are part of the "Ask anything" layer.
- Privacy controls: the site promises zero retention in the cloud, no training on user data, and on-device history storage.

## Important inference

I am inferring that Typeless uses at least some remote AI processing because its homepage explicitly mentions "zero data retention in the cloud" and its "quick answers & actions" copy describes web-connected assistant behavior. At the same time, its help and Windows marketing pages say history stays local on-device. That suggests a hybrid architecture rather than a fully on-device stack.

## Key UX patterns worth porting

These are the parts of Typeless that fit WhisperWindows especially well:

1. A product loop, not a utility screen

Typeless presents one simple loop:

- speak
- clean the language
- paste polished text back where the user is already working

WhisperWindows should present the same loop, but with local inference instead of cloud-backed cleanup.

2. Cross-app feeling

The site repeatedly reinforces that dictation works inside the user's existing tools rather than inside a dedicated editor. That is important for this project because the clipboard-restore and paste flow is already aligned with that idea.

3. A recorder bar instead of a settings-heavy UI

Typeless centers a compact dictation bar and lightweight floating UI. That matches WhisperWindows better than a heavy window or dashboard.

4. Personal writing adaptation

Typeless treats vocabulary, phrasing, and tone as first-class quality features. For WhisperWindows, this suggests:

- a local personal dictionary
- per-app cleanup profiles
- optional local LLM rewriting after Whisper transcription

5. Privacy as a product feature

Typeless markets privacy; WhisperWindows can go further by making local execution the default architecture rather than a cloud-retention promise.

## Typeless -> WhisperWindows mapping

| Typeless pillar | Public promise | Local-first WhisperWindows version |
| --- | --- | --- |
| Speak, don't type | Real-time polished writing from voice | Hotkey dictation with local Whisper transcription |
| Works everywhere | Cross-app voice input | Paste into the previously focused app with clipboard restore |
| AI auto-editing | Remove filler, repetition, and formatting friction | Local cleanup pass after transcription |
| Personalized style and tone | Writing that sounds like you | Optional per-app or per-profile local rewrite prompts |
| Personal dictionary | Preserve names and jargon | Local glossary with manual and auto-added entries |
| 100+ languages | Multilingual dictation and language detection | Mixed Korean/English first, then broader language support |
| Ask anything | Edit selected text, summarize, translate, answer questions | Local selected-text actions backed by a local LLM |
| Private by design | Zero cloud retention, on-device history | Fully local inference, local history, local glossary |

## Recommended implementation order

### Phase 1: Product language and UI shell

- Reframe the UI around "speak -> polish -> paste"
- Show app-context and privacy visually
- Keep the recorder surface compact and fast

### Phase 2: Local cleanup pipeline

- Post-process Whisper output to remove filler and repetition
- Normalize punctuation and list formatting
- Add "change my mind" cleanup heuristics

### Phase 3: Local personalization

- Add a local dictionary for names, company terms, and bilingual vocabulary
- Track accepted corrections to improve future suggestions
- Add per-app tone presets such as chat, email, and docs

### Phase 4: Local selected-text actions

- Rewrite
- shorten
- translate
- summarize
- answer questions about highlighted text

## Concrete product direction

The best version of this project is not "Typeless, but Windows." It is:

"A Typeless-inspired local voice layer for Windows that uses Whisper plus optional local LLM cleanup to turn speech into polished writing in any app."

That framing is a better fit for the existing architecture and gives the project a clear differentiator.
