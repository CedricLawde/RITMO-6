//! Pattern memory and live state shared between the audio thread and the editor.

use crate::params::{NUM_CH, NUM_PROGS, NUM_STEPS};
use crate::presets;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI8, AtomicU16, AtomicU32, AtomicU8, Ordering::Relaxed};

/// What a click on a pad does (PAD MODE display under the grid).
pub const PAD_NOTE: u8 = 0;
pub const PAD_MULTI: u8 = 1;
pub const PAD_ACCENT: u8 = 2;
pub const PAD_PROB: u8 = 3;
pub const PAD_LENGTH: u8 = 4;
pub const NUM_PAD_MODES: u8 = 5;
pub const PAD_MODE_NAMES: [&str; 5] = ["NOTE", "MULTI", "ACCENT", "PROB", "LENGTH"];

/// Chance of a step, by its 2-bit code: 0 = always (the default, so older files play as before).
pub const PROB_PERCENT: [u32; 4] = [100, 75, 50, 25];

pub struct SharedState {
    pub steps: [[AtomicU16; NUM_CH]; NUM_PROGS],
    /// Multiply: hits per step minus one, 2 bits per step (0 = one hit .. 3 = four hits).
    pub multi: [[AtomicU32; NUM_CH]; NUM_PROGS],
    /// ACCENT: one bit per step.
    pub accent: [[AtomicU16; NUM_CH]; NUM_PROGS],
    /// PROB: chance of each step, 2 bits per step (see PROB_PERCENT).
    pub prob: [[AtomicU32; NUM_CH]; NUM_PROGS],
    /// PAD MODE (PAD_NOTE .. PAD_LENGTH).
    pub pad_mode: AtomicU8,
    /// Program length (STEPS row). Setting it sets every voice's length too.
    pub lengths: [AtomicU8; NUM_PROGS],
    /// Length of each voice's own loop (polymetry), 1..=16.
    pub voice_lengths: [[AtomicU8; NUM_CH]; NUM_PROGS],
    /// Step each voice is playing, -1 when stopped.
    pub voice_step: [AtomicI8; NUM_CH],
    pub program: AtomicU8,
    /// Internal start/stop (START lever or MIDI start keys).
    pub running: AtomicBool,
    /// Step shown as playing, -1 when stopped.
    pub cur_step: AtomicI32,
    /// Bit mask of soloed rows (ONE buttons). 0 = all rows play.
    pub solo: AtomicU8,
    /// 0 = all channels, 1..=16.
    pub midi_channel: AtomicU8,
    /// Keyboard control of programs and start/stop.
    pub keyb_ctrl: AtomicBool,
    /// Bit mask of voices the editor wants to hear now (TRIG buttons).
    pub trig_req: AtomicU8,
    /// Incremented every time a voice is struck (drives the panel lamps).
    pub hits: [AtomicU32; NUM_CH],
    /// True when the host transport is driving the sequencer.
    pub host_driven: AtomicBool,
    /// Last tempo used, as f32 bits (for display).
    pub bpm_bits: AtomicU32,
    /// Factory kit last loaded (0..16).
    pub kit: AtomicU8,
    /// KICK button of the spring reverb (taken by the audio thread).
    pub spring_kick: AtomicBool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PatternData {
    pub steps: Vec<Vec<u16>>,
    pub lengths: Vec<u8>,
    pub program: u8,
    pub midi_channel: u8,
    pub keyb_ctrl: bool,
    #[serde(default)]
    pub kit: u8,
    #[serde(default)]
    pub multi: Vec<Vec<u32>>,
    /// v1 files: MULTIPLY switch. Read only, replaced by `pad_mode`.
    #[serde(default, skip_serializing)]
    pub multiply_mode: bool,
    #[serde(default)]
    pub accent: Vec<Vec<u16>>,
    #[serde(default)]
    pub prob: Vec<Vec<u32>>,
    /// Empty in v1 files: every voice then follows its program length.
    #[serde(default)]
    pub voice_lengths: Vec<Vec<u8>>,
    #[serde(default)]
    pub pad_mode: Option<u8>,
}

impl Default for SharedState {
    fn default() -> Self {
        let s = SharedState {
            steps: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU16::new(0))),
            multi: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
            accent: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU16::new(0))),
            prob: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
            pad_mode: AtomicU8::new(PAD_NOTE),
            lengths: std::array::from_fn(|_| AtomicU8::new(16)),
            voice_lengths: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU8::new(16))),
            voice_step: std::array::from_fn(|_| AtomicI8::new(-1)),
            program: AtomicU8::new(0),
            running: AtomicBool::new(false),
            cur_step: AtomicI32::new(-1),
            solo: AtomicU8::new(0),
            midi_channel: AtomicU8::new(0),
            keyb_ctrl: AtomicBool::new(true),
            trig_req: AtomicU8::new(0),
            hits: std::array::from_fn(|_| AtomicU32::new(0)),
            host_driven: AtomicBool::new(false),
            bpm_bits: AtomicU32::new(120f32.to_bits()),
            kit: AtomicU8::new(0),
            spring_kick: AtomicBool::new(false),
        };
        s.load_factory();
        s
    }
}

impl SharedState {
    pub fn load_factory(&self) {
        for (i, p) in presets::default_programs().into_iter().enumerate().take(NUM_PROGS) {
            for c in 0..NUM_CH {
                self.steps[i][c].store(p.rows[c], Relaxed);
            }
            self.set_length(i, p.length as usize);
        }
        // Demo of Multiply in program 1: hat with 2 and 3 hits, a 4-hit roll on row A.
        let set = |ch: usize, step: usize, n: u32| {
            self.set_step(0, ch, step, true);
            for _ in 1..n {
                self.cycle_hits(0, ch, step);
            }
        };
        set(4, 12, 2);
        set(4, 14, 3);
        set(0, 15, 4);
        // v2 demo in program 1: an accented downbeat, two hat steps left to chance, and row D
        // on its own 12-step loop (three hits a dotted quarter apart) against the 16 steps
        if self.step_on(0, 0, 0) {
            self.toggle_accent(0, 0, 0);
        }
        for (step, clicks) in [(6usize, 2usize), (10, 1)] {
            if self.step_on(0, 4, step) {
                for _ in 0..clicks {
                    self.cycle_prob(0, 4, step);
                }
            }
        }
        for s in [0usize, 3, 6, 9] {
            self.set_step(0, 3, s, true);
        }
        self.set_voice_length(0, 3, 12);
    }

    pub fn program(&self) -> usize {
        (self.program.load(Relaxed) as usize).min(NUM_PROGS - 1)
    }

    pub fn length(&self, prog: usize) -> usize {
        (self.lengths[prog].load(Relaxed) as usize).clamp(1, NUM_STEPS)
    }

    /// STEPS row: the program length, given to every voice.
    pub fn set_length(&self, prog: usize, len: usize) {
        let l = len.clamp(1, NUM_STEPS) as u8;
        self.lengths[prog].store(l, Relaxed);
        for c in 0..NUM_CH {
            self.voice_lengths[prog][c].store(l, Relaxed);
        }
    }

    pub fn voice_length(&self, prog: usize, ch: usize) -> usize {
        (self.voice_lengths[prog][ch].load(Relaxed) as usize).clamp(1, NUM_STEPS)
    }

    /// LENGTH mode: one voice's own loop. The program length (STEPS row, step numbers) follows
    /// the longest voice.
    pub fn set_voice_length(&self, prog: usize, ch: usize, len: usize) {
        self.voice_lengths[prog][ch].store(len.clamp(1, NUM_STEPS) as u8, Relaxed);
        let longest = (0..NUM_CH).map(|c| self.voice_length(prog, c)).max().unwrap_or(16);
        self.lengths[prog].store(longest as u8, Relaxed);
    }

    pub fn accent_on(&self, prog: usize, ch: usize, step: usize) -> bool {
        self.accent[prog][ch].load(Relaxed) & (1 << step) != 0
    }

    /// ACCENT mode click: an off step comes on with an accent; an on step toggles its accent.
    pub fn toggle_accent(&self, prog: usize, ch: usize, step: usize) {
        if !self.step_on(prog, ch, step) {
            self.set_step(prog, ch, step, true);
            self.accent[prog][ch].fetch_or(1 << step, Relaxed);
        } else {
            self.accent[prog][ch].fetch_xor(1 << step, Relaxed);
        }
    }

    /// Chance of a step in percent (100, 75, 50 or 25).
    pub fn prob_at(&self, prog: usize, ch: usize, step: usize) -> u32 {
        PROB_PERCENT[((self.prob[prog][ch].load(Relaxed) >> (step * 2)) & 3) as usize]
    }

    fn set_prob_code(&self, prog: usize, ch: usize, step: usize, code: u32) {
        let shift = step * 2;
        let cur = self.prob[prog][ch].load(Relaxed);
        self.prob[prog][ch].store((cur & !(3u32 << shift)) | ((code & 3) << shift), Relaxed);
    }

    /// PROB mode click: an off step comes on at 100 %, then 75, 50, 25 and back to 100 %.
    pub fn cycle_prob(&self, prog: usize, ch: usize, step: usize) {
        if !self.step_on(prog, ch, step) {
            self.set_step(prog, ch, step, true);
            return;
        }
        let code = (self.prob[prog][ch].load(Relaxed) >> (step * 2)) & 3;
        self.set_prob_code(prog, ch, step, (code + 1) % 4);
    }

    /// RESET PRG: steps, MULTIPLY, ACCENT and PROB cleared, every length back to 16.
    pub fn clear_program(&self, prog: usize) {
        for c in 0..NUM_CH {
            self.steps[prog][c].store(0, Relaxed);
            self.multi[prog][c].store(0, Relaxed);
            self.accent[prog][c].store(0, Relaxed);
            self.prob[prog][c].store(0, Relaxed);
        }
        self.set_length(prog, 16);
    }

    pub fn step_on(&self, prog: usize, ch: usize, step: usize) -> bool {
        self.steps[prog][ch].load(Relaxed) & (1 << step) != 0
    }

    pub fn set_step(&self, prog: usize, ch: usize, step: usize, on: bool) {
        if on {
            self.steps[prog][ch].fetch_or(1 << step, Relaxed);
        } else {
            self.steps[prog][ch].fetch_and(!(1u16 << step), Relaxed);
            // an off step forgets its MULTIPLY, ACCENT and PROB
            self.set_hits(prog, ch, step, 1);
            self.accent[prog][ch].fetch_and(!(1u16 << step), Relaxed);
            self.set_prob_code(prog, ch, step, 0);
        }
    }

    /// Hits played inside one step (1..=4); 0 when the step is off.
    pub fn hits_at(&self, prog: usize, ch: usize, step: usize) -> u32 {
        if !self.step_on(prog, ch, step) {
            return 0;
        }
        ((self.multi[prog][ch].load(Relaxed) >> (step * 2)) & 3) + 1
    }

    fn set_hits(&self, prog: usize, ch: usize, step: usize, n: u32) {
        let shift = step * 2;
        let bits = (n.clamp(1, 4) - 1) << shift;
        let cur = self.multi[prog][ch].load(Relaxed);
        self.multi[prog][ch].store((cur & !(3u32 << shift)) | bits, Relaxed);
    }

    /// Multiply mode click: off -> 1 -> 2 -> 3 -> 4 -> off.
    pub fn cycle_hits(&self, prog: usize, ch: usize, step: usize) {
        match self.hits_at(prog, ch, step) {
            0 => {
                self.set_hits(prog, ch, step, 1);
                self.set_step(prog, ch, step, true);
            }
            4 => self.set_step(prog, ch, step, false),
            n => self.set_hits(prog, ch, step, n + 1),
        }
    }

    pub fn snapshot(&self) -> PatternData {
        PatternData {
            steps: (0..NUM_PROGS)
                .map(|p| (0..NUM_CH).map(|c| self.steps[p][c].load(Relaxed)).collect())
                .collect(),
            lengths: (0..NUM_PROGS).map(|p| self.lengths[p].load(Relaxed)).collect(),
            program: self.program.load(Relaxed),
            midi_channel: self.midi_channel.load(Relaxed),
            keyb_ctrl: self.keyb_ctrl.load(Relaxed),
            kit: self.kit.load(Relaxed),
            multi: (0..NUM_PROGS)
                .map(|p| (0..NUM_CH).map(|c| self.multi[p][c].load(Relaxed)).collect())
                .collect(),
            multiply_mode: false,
            accent: (0..NUM_PROGS)
                .map(|p| (0..NUM_CH).map(|c| self.accent[p][c].load(Relaxed)).collect())
                .collect(),
            prob: (0..NUM_PROGS)
                .map(|p| (0..NUM_CH).map(|c| self.prob[p][c].load(Relaxed)).collect())
                .collect(),
            voice_lengths: (0..NUM_PROGS)
                .map(|p| (0..NUM_CH).map(|c| self.voice_lengths[p][c].load(Relaxed)).collect())
                .collect(),
            pad_mode: Some(self.pad_mode.load(Relaxed)),
        }
    }

    pub fn restore(&self, d: &PatternData) {
        for p in 0..NUM_PROGS {
            for c in 0..NUM_CH {
                let v = d.steps.get(p).and_then(|r| r.get(c)).copied().unwrap_or(0);
                self.steps[p][c].store(v, Relaxed);
                let m = d.multi.get(p).and_then(|r| r.get(c)).copied().unwrap_or(0);
                self.multi[p][c].store(m, Relaxed);
                let a = d.accent.get(p).and_then(|r| r.get(c)).copied().unwrap_or(0);
                self.accent[p][c].store(a, Relaxed);
                let q = d.prob.get(p).and_then(|r| r.get(c)).copied().unwrap_or(0);
                self.prob[p][c].store(q, Relaxed);
            }
            let l = d.lengths.get(p).copied().unwrap_or(16).clamp(1, 16);
            self.lengths[p].store(l, Relaxed);
            for c in 0..NUM_CH {
                // v1 files have no voice lengths: every voice follows its program
                let vl = d.voice_lengths.get(p).and_then(|r| r.get(c)).copied().unwrap_or(l).clamp(1, 16);
                self.voice_lengths[p][c].store(vl, Relaxed);
            }
        }
        self.program.store(d.program.min(NUM_PROGS as u8 - 1), Relaxed);
        self.midi_channel.store(d.midi_channel.min(16), Relaxed);
        self.keyb_ctrl.store(d.keyb_ctrl, Relaxed);
        self.kit.store(d.kit.min(16), Relaxed);
        let mode = d.pad_mode.unwrap_or(if d.multiply_mode { PAD_MULTI } else { PAD_NOTE });
        self.pad_mode.store(mode.min(NUM_PAD_MODES - 1), Relaxed);
    }

    /// Whether a row may sound given mute flags and the ONE (solo) buttons.
    pub fn row_audible(&self, ch: usize, muted: bool) -> bool {
        if muted {
            return false;
        }
        let solo = self.solo.load(Relaxed);
        solo == 0 || solo & (1 << ch) != 0
    }
}
