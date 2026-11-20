//! Pattern memory and live state shared between the audio thread and the editor.

use crate::params::{NUM_CH, NUM_PROGS, NUM_STEPS};
use crate::presets;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU16, AtomicU32, AtomicU8, Ordering::Relaxed};

pub struct SharedState {
    pub steps: [[AtomicU16; NUM_CH]; NUM_PROGS],
    /// Multiply: hits per step minus one, 2 bits per step (0 = one hit .. 3 = four hits).
    pub multi: [[AtomicU32; NUM_CH]; NUM_PROGS],
    /// Multiply mode: a click on a pad cycles one, two, three, four hits, then off.
    pub multiply_mode: AtomicBool,
    pub lengths: [AtomicU8; NUM_PROGS],
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
    #[serde(default)]
    pub multiply_mode: bool,
}

impl Default for SharedState {
    fn default() -> Self {
        let s = SharedState {
            steps: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU16::new(0))),
            multi: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
            multiply_mode: AtomicBool::new(false),
            lengths: std::array::from_fn(|_| AtomicU8::new(16)),
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
            self.lengths[i].store(p.length.clamp(1, 16), Relaxed);
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
    }

    pub fn program(&self) -> usize {
        (self.program.load(Relaxed) as usize).min(NUM_PROGS - 1)
    }

    pub fn length(&self, prog: usize) -> usize {
        (self.lengths[prog].load(Relaxed) as usize).clamp(1, NUM_STEPS)
    }

    pub fn step_on(&self, prog: usize, ch: usize, step: usize) -> bool {
        self.steps[prog][ch].load(Relaxed) & (1 << step) != 0
    }

    pub fn set_step(&self, prog: usize, ch: usize, step: usize, on: bool) {
        if on {
            self.steps[prog][ch].fetch_or(1 << step, Relaxed);
        } else {
            self.steps[prog][ch].fetch_and(!(1u16 << step), Relaxed);
            self.set_hits(prog, ch, step, 1);
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
            multiply_mode: self.multiply_mode.load(Relaxed),
        }
    }

    pub fn restore(&self, d: &PatternData) {
        for p in 0..NUM_PROGS {
            for c in 0..NUM_CH {
                let v = d.steps.get(p).and_then(|r| r.get(c)).copied().unwrap_or(0);
                self.steps[p][c].store(v, Relaxed);
                let m = d.multi.get(p).and_then(|r| r.get(c)).copied().unwrap_or(0);
                self.multi[p][c].store(m, Relaxed);
            }
            let l = d.lengths.get(p).copied().unwrap_or(16).clamp(1, 16);
            self.lengths[p].store(l, Relaxed);
        }
        self.program.store(d.program.min(NUM_PROGS as u8 - 1), Relaxed);
        self.midi_channel.store(d.midi_channel.min(16), Relaxed);
        self.keyb_ctrl.store(d.keyb_ctrl, Relaxed);
        self.kit.store(d.kit.min(16), Relaxed);
        self.multiply_mode.store(d.multiply_mode, Relaxed);
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
