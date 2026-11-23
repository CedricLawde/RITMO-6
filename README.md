# Ritmo-6 — by Geranium

**Free 6-voice analog-style drum machine — VST3 · CLAP · standalone (Windows 64-bit)**
*Boîte à rythmes analogique 6 voix, gratuite — VST3 · CLAP · autonome (Windows 64 bits)*

![Ritmo-6 user manual](manual/user-manual-ritmo6.png)

*Mode d'emploi en français : [manual/mode-emploi-ritmo6.png](manual/mode-emploi-ritmo6.png)*

## Features / Fonctions
- 6 synthesized voices (tone oscillator with pitch sweep + filtered noise), each with its own sound module, RAND, LOAD/SAVE
- Two sound engines: **MINI** (punchy, Minipops-style) and **ORIG**
- 16-step × 6-row sequencer, 16 programs, pattern length 1–16, RESET / COPY / PASTE / RANDOM
- **PAD MODE** (v2): a click on a step can switch it (NOTE), split it (MULTI), accent it (ACCENT), set its chance (PROB) or end the voice's loop (LENGTH)
- **MULTIPLY**: 1, 2, 3 or 4 hits inside a step (ratchets), shown in green
- **ACCENT** (v2): accented steps (red bar) play louder and a little hotter; one ACCENT knob, up to +4 dB
- **PROBABILITY** (v2): 100, 75, 50 or 25 % chance per step, drawn on every pass; the pad is lit up to its chance
- **Per-voice length / polymetry** (v2): each voice loops over its own 1-16 steps; the STEPS row sets them all
- **PAN RND**: random panning per hit
- Mixer with volume, pan, trig, mute, solo (ONE/ALL); stereo mix + 6 separate outputs
- Internal tempo or host sync (1/8, 1/8T, 1/16, 1/32); MIDI keyboard control
- **TEMPO DRIFT**, **SOUND DRIFT** and **SWING**; per-voice **NUDGE** (up to half a step early or late) and drift amount
- **FX panel** behind the front plate (press FX): phaser, op-amp distortion, delay (TAPE / BBD / MOD, sync or free) and spring reverb with KICK; 4 sends per voice. The separate outputs stay dry (see [ROUTAGE.md](ROUTAGE.md) for the routing).
- Spring reverb **LOW CUT / HIGH CUT**; the window opens 1.6x larger and can be resized (plugin too)

## Install / Installation
Download the latest files from the **Releases** page:
- `Ritmo-6.vst3` → copy to `C:\Program Files\Common Files\VST3`
- `Ritmo-6.clap` → copy to `C:\Program Files\Common Files\CLAP`
- `Ritmo-6.exe` → standalone app (space bar = start/stop)

Then rescan plugins in your DAW. The plugin appears under the vendor **Geranium**.

MIDI: notes 36–41 = voices A–F · 42–57 = programs 1–16 · 58+ = hold to play.

Upgrading from 1.x: v2 keeps the same plugin ID and parameter names, so it replaces 1.x in your DAW
and your projects, banks (.json), pattern files (.r6p) and pattern sets (.r6s) open unchanged (no accent, 100 % chance,
one length per program). Files saved by v2 carry the new data; v1 ignores it.

## Build from source
Requires Rust (stable). On Windows:
```
cargo build --release
```
`target/release/ritmo6_vst3.dll` is the plugin (rename it to `Ritmo-6.vst3` or `Ritmo-6.clap`);
`target/release/ritmo6.exe` is the standalone app. `bundle.ps1` does both and fills `dist/`.

## Credits
- Inspired by the freeware **Ocet** rhythm machine by Elektrostudio (2009) and the vintage
  EKO ComputeRhythm. Ritmo-6 is an independent project, not affiliated with either.
- Built with [nih-plug](https://github.com/robbert-vdh/nih-plug), [egui](https://github.com/emilk/egui),
  cpal and midir. Panel chassis rendered with Blender.

## License
GPL-3.0 — see [LICENSE](LICENSE). (The VST3 bindings used by nih-plug are GPL-3.0, so the
plugin is distributed under the same license.)
