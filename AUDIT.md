# Ritmo-6 v2.0.0 (2026-11-23) — probability, accent, per-voice length

| Change | Behaviour | Verified by |
|---|---|---|
| PAD MODE replaces the MULTIPLY switch | Red display with arrows under the grid: NOTE, MULTI, ACCENT, PROB, LENGTH. A click on the display steps to the next mode. v1 banks saved with MULTIPLY on open in MULTI. | test `v1_files_still_load`; screenshot |
| ACCENT | One bit per step (red bar on the pad). An accented hit plays x(1 + 0.6 x ACCENT) (up to +4 dB) and adds up to 0.5 extra unit of retrigger charge, so it warms a little on its own and pushes rolls further into the saturation of v1.3.4. New automatable parameter `accent` (default 50 %), placed last so v1 parameter numbering is kept. | test `accent_plays_louder` |
| PROB | 2 bits per step: 100 / 75 / 50 / 25 %. Drawn once per step and voice when the step is reached (when it is queued, for an early-NUDGE voice); a lost draw silences the whole step, MULTIPLY hits included. Clearing a step resets its chance and accent. The pad is lit only up to its chance. | test `probability_thins_out_a_step` (800 steps: 100 % -> 800, 50 % -> 330-470, 25 % -> 140-260) |
| Per-voice length (polymetry) | Each voice has its own loop of 1-16 steps and its own cursor; LENGTH mode sets one voice, the STEPS row sets all of them. The program length (step numbers, STEPS row) follows the longest voice. Internal clock: each voice advances by one step; host sync: each voice's step is the host step modulo its length, so it stays in place after a jump or loop. | tests `each_voice_loops_over_its_own_length`, `host_sync_places_each_voice_in_its_own_loop` |
| Files | Banks and pattern files gain `accent`, `prob`, `voice_lengths`, `pad_mode` (all optional). v1 files load with no accent, 100 % and one length per program. COPY / PASTE PRG carry the new data. | tests `v1_files_still_load`, `v2_fields_round_trip` |
| Factory program 1 demo | Accent on A1, steps E7 (50 %) and E11 (75 %), row D on a 12-step loop (steps 1, 4, 7, 10). | screenshot |

Not yet checked: listening in a host by the person. Manuals (HTML and PNG, FR and EN) redrawn with the v2 panel.

---

# Ritmo-6 v1.3.5 (2026-11-23) — crash on LOAD / SAVE in the plugin

| Finding | Fix | Verified by |
|---|---|---|
| Clicking LOAD or SAVE (bank or module) in the plugin crashed the host with 0xc0000409 (fail-fast) in Reaper, Cubase, VSTHost and Ableton Live. The Windows file dialog was opened from inside the plugin window's frame; its modal message loop called back into the window code that was still drawing, which stopped the process. Ableton's log of the reported crash shows v1.3.1 loaded, before this fix. | File dialogs now run on their own thread; the bank or module text is prepared before (SAVE) or applied after (LOAD) on the GUI thread, picked up on a later frame. Only one file window at a time. The plugin's panel drawing is also wrapped so a panic skips a frame instead of aborting the host. | test `file_jobs_finish_off_the_gui_thread`; build. Not yet confirmed in a host by the person. |

---

# Ritmo-6 v1.3.4 (2026-11-23) — retrigger saturation

| Finding | Fix | Verified by |
|---|---|---|
| Saturation came only from MULTIPLY hits and switched fully on at the 2nd hit of a step (charge 1 -> ~2, blend 100 %); normal steps reset the charge, however fast. | Every hit adds one unit of charge to what is left; it leaks with a fifth of the voice's decay (15-300 ms); only the excess over 1 drives a faded-in soft overload into a knee-based ceiling (clean to 0.9, rounding to 1.6), so the level only rises. Documented in SATURATION.md. | test `multiply_hits_saturate`: kick, 4th-hit peak / lone hit = 1.00 (1 s, 250 ms), 1.08 (125 ms), 1.23 (60 ms), 1.57 (30 ms), 1.63 (15 ms) |

---

# Ritmo-6 v1.3.3 (2026-11-23)

| Change | Behaviour | Verified by |
|---|---|---|
| No console window | The standalone is built as a Windows GUI program (`windows_subsystem = "windows"`), so no black console window opens behind the panel. The plugin is unaffected. | PE header subsystem = 2 (Windows GUI); screenshot run |

---

# Ritmo-6 v1.3.2 (2026-11-23) — phaser analysis

| Finding | Fix | Verified by |
|---|---|---|
| The phaser is a send effect, but its return held 50 % dry + 50 % phase-shifted signal, scaled by MIX (default 50 %). Added to the dry voice already in the Mix, that is 1.75 x dry + 0.25 x shifted at full send: the notches were only about 2.5 dB deep, hence the "timid" sound. | The return now carries only the phase-shifted signal (x MIX x LEVEL). Dry in the Mix + return at send 100 %, MIX 100 %, LEVEL 0 dB are equal, so the notches cancel completely. MIX default 100 %. | test `phaser_notches_cut_deep_against_the_dry_mix` (notch deeper than -20 dB on a 1 kHz tone) |
| The sweep stopped at 1.6 kHz at the default DEPTH (4.5 kHz at maximum), below hats and cymbals. | Corner now 160 Hz to 10 kHz at full DEPTH (default 75 % reaches 3.6 kHz, notches up to about 9 kHz). | code review, same test |
| Both channels moved almost together (11 degrees apart). | Right channel 36 degrees behind: the sweep moves across the stereo image. COLOR feedback up to 0.92 (was 0.8), default 45 %. | listening still to be done by the person |

---

# Ritmo-6 v1.3.1 (2026-11-23)

| Change | Behaviour | Verified by |
|---|---|---|
| MUTE acts on the Mix only | A muted voice still plays (and lights its lamp); it is left out of the stereo Mix together with its FX sends, while its separate output keeps playing. A voice routed to its own DAW track is therefore never doubled in the Mix. The standalone app (Mix only) behaves as before. | test `mute_leaves_the_mix_but_keeps_the_voice_output` |

---

# Ritmo-6 v1.3.0 (2026-11-23)

| Change | Behaviour | Verified by |
|---|---|---|
| Spring LOW CUT / HIGH CUT | 12 dB/octave high-pass (20 Hz = off .. 1 kHz) and low-pass (1 kHz .. 20 kHz = off) on the reverb return only. New host parameters `sp_lowcut`, `sp_highcut`; defaults are off, so older projects and banks sound the same. | build; 18 tests pass; FX panel screenshot |
| Window size | Standalone and plugin open at 1.6x (1344 x 784); the standalone shrinks once to fit a smaller screen. | screenshot 1344 x 784 |
| Routing | Documented in ROUTAGE.md (each voice is in the Mix and on its own output; the separate outputs are dry, pre-pan, unaffected by MASTER VOL). | code review of `Engine::next_frame` and the plugin's output ports |

---

# Ritmo-6 v1.2.0 — FX expansion audit (2026-11-22)

## New in 1.2.0
| Feature | Behaviour | Verified by |
|---|---|---|
| Four shared FX buses | PHASER (4 all-pass stages, LFO 0.03–12 Hz, COLOR = feedback), DISTORTION (op-amp style: pre-emphasis, gain-dependent bandwidth, 4× oversampled clipping, FILTER low-pass, FAT keeps the lows), DELAY (TAPE / BBD / MOD, SYNC in 13 note values or FREE 20 ms–2 s, feedback with per-mode colour), SPRING REVERB (3 springs of 24 dispersive all-pass stages, DECAY, DAMPING, TENSION, DRIVE, KICK). Each has MIX, LEVEL and BYPASS. Buses never feed each other. | tests `silent_buses_stay_silent`, `every_processor_is_stable_at_extremes` (every knob at maximum, all 3 delay modes, 4 s, finite and bounded), `delay_repeats_at_the_set_time` (1/4 at 120 BPM lands at 500 ms ± 1.25 ms), `a_sleeping_spring_still_answers_the_kick` |
| Sends | 4 sends per voice (PH DS DL SP), taken after the voice's fader and pan, into the four buses; the returns are added to the stereo mix only. The 6 separate outputs stay dry. | code review of `Engine::next_frame` |
| Idle effects | A processor with no input and a silent tail for 0.5 s sleeps until something is sent to it (or the spring is kicked), so FX cost no CPU when unused. | `a_sleeping_spring_still_answers_the_kick`, `delay_repeats_at_the_set_time` (wakes after 2 s of silence) |
| NUDGE (column A) | Moves one voice up to half a step early or late. Early hits are queued from the previous step, so they really sound before the grid (disco claps before the snare). | `nudge_moves_a_voice_early_or_late` |
| DRIFT (column B) | This voice's share of SOUND DRIFT (0–100 %, default 100 % = the 1.1 behaviour). | code review |
| SWING | 50 % (straight) to 75 %: every second 16th is pushed late. Applied on the grid, so it combines with TEMPO DRIFT and NUDGE. | `swing_delays_every_second_step` |
| FX panel | The FX button turns the main plate over around a horizontal hinge (0.65 s, slow start, quick swing, small settle), showing the circuit board inside, then the effects on its back. Drawn in perspective; nothing on the turning plate can be clicked while it moves; no effect on the sound. | screenshots at 30 %, 47 % and 62 % of the turn |
| Program icon | The person's RITMO·6 icon in the window title bar, the task bar and on `Ritmo-6.exe`. | build (Windows resource) |

| Delay noise fix | The simulated tape/BBD hiss was injected inside the repeat loop, and the tape saturation and the 8-bit MOD loop had a gain at or above 1, so at full FEEDBACK/AGE a noise wash or buzz kept going on its own. Hiss now rides on the repeats only, proportional to their level; the loop gain stays under 1. | test `delay_at_full_feedback_dies_away_without_leftover_noise` (all 3 modes, FEEDBACK and AGE at max: below 5 % after 40 s) |

## Compatibility
- Existing parameter IDs unchanged; new: `nudge`, `drshare`, `sph`, `sds`, `sdl`, `ssp` (per voice), `swing`, and the `ph_*`, `ds_*`, `dl_*`, `sp_*` FX parameters. VST3 class ID and CLAP ID unchanged.
- Projects and banks saved with 1.0–1.1 open with every send at 0 and NUDGE/SWING at 0, DRIFT at 100 %: they sound exactly as before.
- The panel is 840 × 490 (was 760 × 490); it scales to any window size.

## Checks (1.2.0)
| Area | Result |
|---|---|
| Tests | 16 pass (engine, bank, effects). |
| Audio thread | Effect buffers are allocated when the plugin starts or the sample rate changes, never in `process`. Parameter changes are smoothed (15 ms). |
| Build | Release VST3/CLAP and standalone build; only warnings come from the nih-plug export macro. |

---

# Ritmo-6 v1.1.0 — pre-production audit (2026-11-21)

## New in 1.1.0
| Feature | Behaviour | Verified by |
|---|---|---|
| MULTIPLY saturation | Each voice has a "capacitor": a normal hit sets it to 1, every extra MULTIPLY hit in the same step adds 1. It discharges with a time constant of 40 % of the voice's decay (20–400 ms). The excess above 1 pushes the voice into a tanh saturation (more level and drive), like an 808/909 retriggered faster than it can recover. Single hits are untouched. | test `multiply_hits_saturate` (4-hit roll carries >15 % more energy than the same hits without stacking, output stays finite and below 1.5) |
| TEMPO DRIFT (0–100 %) | *Revised in 1.1.2:* two-sided knob, 50 % = no drift (default); towards 0 % steps can only fall late (up to 0.4 step), towards 100 % only early (up to 0.4 step); how far each step goes is set by the random structure below. Tested by `tempo_drift_knob_is_two_sided`. *1.1.1:* The grid keeps the exact tempo; each step plays slightly early or late around its own grid position (up to about ±0.3 step at 100 %): an in-bar pattern (which of the 16 positions lean early/late, 12 %), a slow wave (1–3 cycles per 8 bars, 15 %) and per-step looseness (4 %), all faded by a window that is 0 at every 8-bar boundary. Nothing accumulates, so there is no catch-up step and no audible tempo change. Same in host-sync mode (steps can also play early). New pattern and wave every 8 bars. | tests `tempo_drift_realigns_every_128_steps` (steps leave the grid; every gap 0.4–1.6 steps; gaps around each 8-bar boundary within 10 % of normal; steps 128/256 on the grid) and `host_sync_drift_keeps_every_step_once` |
| SOUND DRIFT (0–100 %) | Each hit draws its own pitch (±1.4 semitones), decay (±0.5 V ≈ x/÷1.4), cutoff (±0.6 V) and level (±25 %) at full setting. Works with both MINI and ORIG engines. | code review; covered by `every_voice_makes_a_finite_sound` |

## Checks
| Area | Result |
|---|---|
| Build | `cargo build --release`: VST3/CLAP + standalone, no warnings from project code. C runtime linked statically (no VC++ redistributable needed). |
| Exports | VST3 `GetPluginFactory`, `InitDll`, `ExitDll`; CLAP `clap_entry`. Vendor "Geranium". |
| Tests | 7 engine tests pass (voices, clock, host sync, MIDI keys, MULTIPLY split, saturation, drift realignment). |
| Audio thread | No allocation, locks or file access in `process`: parameters read into a stack struct per block, pattern memory is atomics. File dialogs only run on the GUI thread. |
| Output safety | Mix clamped to ±4; **fixed**: a non-finite sample now outputs silence and resets the voices instead of reaching the host. |
| MIDI | **Fixed**: releasing a start key (58+) always counts, so switching KEYB CTRL off while holding a key can't leave the sequencer stuck. |
| Host transport | **Fixed**: a drift-delayed hit is cancelled when the host stops, so it can't fire on the next start. |
| Sample rate | Set in `initialize`, voices reset in `reset`; all times are computed from the current rate. |
| State | Patterns, MULTIPLY counts, mode and kit are saved with the host project; all parameters (incl. SOUND, T.DRIFT, S.DRIFT) are host parameters and in bank files. Projects saved with 1.0 open (new parameters take their defaults: 0 %). |
| Parameter IDs | Existing IDs unchanged; new: `tdrift`, `sdrift`. VST3 class ID and CLAP ID unchanged, so hosts see 1.1 as the same plugin. |

## Known limits (not blocking)
- Envelope and pitch conversions call `powf` per sample (about 30 per voice per sample): fine on any recent CPU, could be cached if needed.
- Not tested inside a DAW by the builder; the person tested 1.0 in their DAW.
- Windows SmartScreen will warn about the unsigned `Ritmo-6.exe`.
