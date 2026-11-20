//! Sequencer clock, MIDI key map and mixer.

use crate::dsp::{Voice, VoiceSettings};
use crate::params::{internal_bpm, internal_steps_per_sec, sync_division, ParamId, NUM_CH, SYNC_STEPS_PER_BEAT, VOICE_PARAMS};
use crate::state::SharedState;
use std::sync::atomic::Ordering::Relaxed;

/// First MIDI note of the key map (C1 or C3 depending on the host's octave naming).
pub const BASE_NOTE: u8 = 36;
pub const PROGRAM_NOTE: u8 = BASE_NOTE + 6;
pub const START_NOTE: u8 = PROGRAM_NOTE + 16;


#[derive(Clone, Copy)]
pub struct Values {
    pub level: [f32; NUM_CH],
    pub pan: [f32; NUM_CH],
    pub mute: [bool; NUM_CH],
    pub master: f32,
    pub tempo: f32,
    pub sync: bool,
    /// MINI sound engine (true) or the decoded original (false).
    pub classic: bool,
    pub voices: [VoiceSettings; NUM_CH],
}

impl Values {
    pub fn read(get: impl Fn(ParamId) -> f32) -> Self {
        Values {
            level: std::array::from_fn(|c| get(ParamId::Level(c))),
            pan: std::array::from_fn(|c| get(ParamId::Pan(c))),
            mute: std::array::from_fn(|c| get(ParamId::Mute(c)) >= 0.5),
            master: get(ParamId::Master),
            tempo: get(ParamId::Tempo),
            sync: get(ParamId::Sync) >= 0.5,
            classic: get(ParamId::Sound) < 0.5,
            voices: std::array::from_fn(|c| {
                let arr: [f32; 14] = std::array::from_fn(|i| get(ParamId::Voice(c, VOICE_PARAMS[i])));
                VoiceSettings::from_array(&arr)
            }),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Transport {
    pub playing: bool,
    pub pos_beats: Option<f64>,
    pub tempo: Option<f64>,
}

#[derive(Clone, Copy, Default)]
pub struct Frame {
    pub left: f32,
    pub right: f32,
    pub voices: [f32; NUM_CH],
}

pub struct Engine {
    sr: f32,
    voices: [Voice; NUM_CH],
    counter: f64,
    step: usize,
    was_running: bool,
    last_abs_step: i64,
    held_start_keys: u32,
    pan_random: [f32; NUM_CH],
    /// Multiply: hits still to play inside the current step, and samples until the next one.
    pending: [u32; NUM_CH],
    countdown: [f64; NUM_CH],
    interval: [f64; NUM_CH],
    /// Length of the current step in samples.
    step_samples: f64,
    // per block
    host_active: bool,
    block_pos0: f64,
    beats_per_sample: f64,
    host_tempo: f64,
    frame_in_block: u64,
}

impl Engine {
    pub fn new(sample_rate: f32) -> Self {
        Engine {
            sr: sample_rate.max(8000.0),
            voices: std::array::from_fn(|i| Voice::new(0x9E37_79B9u32.wrapping_mul(i as u32 + 1))),
            counter: 0.0,
            step: 0,
            was_running: false,
            last_abs_step: i64::MIN,
            held_start_keys: 0,
            pan_random: [0.0; NUM_CH],
            pending: [0; NUM_CH],
            countdown: [0.0; NUM_CH],
            interval: [0.0; NUM_CH],
            step_samples: 0.0,
            host_active: false,
            block_pos0: 0.0,
            beats_per_sample: 0.0,
            host_tempo: 120.0,
            frame_in_block: 0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr.max(8000.0);
    }

    pub fn reset(&mut self) {
        self.voices = std::array::from_fn(|i| Voice::new(0x9E37_79B9u32.wrapping_mul(i as u32 + 1)));
        self.counter = 0.0;
        self.last_abs_step = i64::MIN;
        self.held_start_keys = 0;
        self.pending = [0; NUM_CH];
    }

    fn channel_ok(state: &SharedState, channel: u8) -> bool {
        let want = state.midi_channel.load(Relaxed);
        want == 0 || want == channel + 1
    }

    /// `channel` is 0-based. Velocity is ignored, as in the original (its envelopes have a fixed
    /// 10 V level), but kept in the signature for hosts.
    pub fn note_on(&mut self, state: &SharedState, channel: u8, note: u8, _velocity: f32) {
        if !Self::channel_ok(state, channel) || note < BASE_NOTE {
            return;
        }
        if note < PROGRAM_NOTE {
            let ch = (note - BASE_NOTE) as usize;
            self.strike(state, ch);
        } else if !state.keyb_ctrl.load(Relaxed) {
        } else if note < START_NOTE {
            state.program.store(note - PROGRAM_NOTE, Relaxed);
        } else {
            self.held_start_keys += 1;
            state.running.store(true, Relaxed);
        }
    }

    pub fn note_off(&mut self, state: &SharedState, channel: u8, note: u8) {
        if !Self::channel_ok(state, channel) || note < START_NOTE || !state.keyb_ctrl.load(Relaxed) {
            return;
        }
        if self.held_start_keys > 0 {
            self.held_start_keys -= 1;
            if self.held_start_keys == 0 {
                state.running.store(false, Relaxed);
            }
        }
    }

    fn strike(&mut self, state: &SharedState, ch: usize) {
        self.voices[ch].trigger(self.pan_random[ch]);
        state.hits[ch].fetch_add(1, Relaxed);
    }

    /// Call once per audio block before pulling frames.
    pub fn begin_block(&mut self, state: &SharedState, vals: &Values, transport: &Transport) {
        let req = state.trig_req.swap(0, Relaxed);
        for ch in 0..NUM_CH {
            if req & (1 << ch) != 0 {
                self.strike(state, ch);
            }
        }
        for ch in 0..NUM_CH {
            self.pan_random[ch] = vals.voices[ch].pan_random;
        }
        if let Some(t) = transport.tempo {
            if t > 1.0 {
                self.host_tempo = t;
            }
        }
        self.host_active = vals.sync && transport.playing && transport.pos_beats.is_some();
        if self.host_active {
            self.block_pos0 = transport.pos_beats.unwrap_or(0.0);
            self.beats_per_sample = self.host_tempo / 60.0 / self.sr as f64;
        } else {
            self.last_abs_step = i64::MIN;
            if !state.running.load(Relaxed) {
                state.cur_step.store(-1, Relaxed);
            }
        }
        self.frame_in_block = 0;
        state.host_driven.store(self.host_active, Relaxed);
        let bpm = if vals.sync { self.host_tempo as f32 } else { internal_bpm(vals.tempo) };
        state.bpm_bits.store(bpm.to_bits(), Relaxed);
    }

    fn fire(&mut self, state: &SharedState, vals: &Values) {
        state.cur_step.store(self.step as i32, Relaxed);
        let prog = state.program();
        for ch in 0..NUM_CH {
            let n = state.hits_at(prog, ch, self.step);
            if n > 0 && state.row_audible(ch, vals.mute[ch]) {
                self.strike(state, ch);
                // Multiply: the step keeps its length and is split into n equal hits.
                self.pending[ch] = n - 1;
                self.interval[ch] = self.step_samples / n as f64;
                self.countdown[ch] = self.interval[ch];
            } else {
                self.pending[ch] = 0;
            }
        }
    }

    fn clock(&mut self, state: &SharedState, vals: &Values) {
        let prog = state.program();
        let len = state.length(prog);
        if self.host_active {
            let spq = SYNC_STEPS_PER_BEAT[sync_division(vals.tempo)];
            self.step_samples = self.sr as f64 * 60.0 / (self.host_tempo * spq);
            let beat = self.block_pos0 + self.frame_in_block as f64 * self.beats_per_sample;
            let abs = (beat * spq + 1e-9).floor() as i64;
            if abs != self.last_abs_step {
                self.last_abs_step = abs;
                self.step = abs.rem_euclid(len as i64) as usize;
                self.fire(state, vals);
            }
            self.was_running = false;
            return;
        }
        let running = state.running.load(Relaxed);
        if running {
            let steps_per_sec = if vals.sync {
                self.host_tempo / 60.0 * SYNC_STEPS_PER_BEAT[sync_division(vals.tempo)]
            } else {
                internal_steps_per_sec(vals.tempo) as f64
            };
            let samples_per_step = self.sr as f64 / steps_per_sec;
            self.step_samples = samples_per_step;
            if !self.was_running {
                self.step = 0;
                self.counter = 0.0;
                self.fire(state, vals);
            } else {
                self.counter += 1.0;
                if self.counter >= samples_per_step {
                    self.counter -= samples_per_step;
                    self.step = (self.step + 1) % len;
                    self.fire(state, vals);
                }
            }
        } else if self.was_running {
            state.cur_step.store(-1, Relaxed);
            self.pending = [0; NUM_CH];
        }
        self.was_running = running;
    }

    pub fn next_frame(&mut self, state: &SharedState, vals: &Values) -> Frame {
        self.clock(state, vals);
        self.frame_in_block += 1;
        for ch in 0..NUM_CH {
            if self.pending[ch] > 0 {
                self.countdown[ch] -= 1.0;
                if self.countdown[ch] <= 0.0 {
                    self.countdown[ch] += self.interval[ch];
                    self.pending[ch] -= 1;
                    if state.row_audible(ch, vals.mute[ch]) {
                        self.strike(state, ch);
                    }
                }
            }
        }
        let mut f = Frame::default();
        for ch in 0..NUM_CH {
            let raw = self.voices[ch].render(&vals.voices[ch], self.sr, vals.classic);
            // Level Adj: linear, 10 V = unity.
            let gain = if vals.mute[ch] { 0.0 } else { vals.level[ch] / 10.0 };
            let v = raw * gain;
            f.voices[ch] = v;
            // Pan -5..+5 V (+ Pan Random offset), equal-power law (estimated).
            let pan = ((vals.pan[ch] + self.voices[ch].pan_offset) / 5.0).clamp(-1.0, 1.0);
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            f.left += v * angle.cos();
            f.right += v * angle.sin();
        }
        let m = vals.master / 10.0;
        f.left = (f.left * m).clamp(-4.0, 4.0);
        f.right = (f.right * m).clamp(-4.0, 4.0);
        f
    }

    pub fn host_tempo(&self) -> f64 {
        self.host_tempo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{spec, ParamId};

    fn defaults() -> Values {
        Values::read(|id: ParamId| spec(id).default)
    }

    #[test]
    fn every_voice_makes_a_finite_sound() {
        let state = SharedState::default();
        let vals = defaults();
        for ch in 0..NUM_CH {
            let mut e = Engine::new(48000.0);
            e.note_on(&state, 0, BASE_NOTE + ch as u8, 1.0);
            e.begin_block(&state, &vals, &Transport::default());
            let mut peak = 0.0f32;
            for _ in 0..24000 {
                let f = e.next_frame(&state, &vals);
                assert!(f.left.is_finite() && f.right.is_finite());
                peak = peak.max(f.voices[ch].abs());
            }
            assert!(peak > 0.02, "voice {ch} too quiet: {peak}");
            assert!(peak < 4.0, "voice {ch} too loud: {peak}");
        }
    }

    #[test]
    fn internal_clock_runs_sixteenths() {
        let state = SharedState::default();
        state.set_step(0, 0, 15, false); // leave out the Multiply demo roll
        let mut vals = defaults();
        vals.tempo = 6.1; // factory tempo 6.1 V -> 8.784 steps/s
        assert!((internal_steps_per_sec(vals.tempo) - 8.784).abs() < 0.001);
        let mut e = Engine::new(48000.0);
        state.running.store(true, Relaxed);
        e.begin_block(&state, &vals, &Transport::default());
        let before = state.hits[0].load(Relaxed);
        // program 1 row A on steps 1, 9, 11 -> 3 hits in one 16-step bar (87 432 samples)
        for _ in 0..87000 {
            e.next_frame(&state, &vals);
        }
        assert_eq!(state.hits[0].load(Relaxed) - before, 3);
    }

    #[test]
    fn host_sync_follows_position() {
        let state = SharedState::default();
        let mut vals = defaults();
        vals.sync = true;
        vals.tempo = 6.0; // 5..6.9 V -> 1/16
        let mut e = Engine::new(48000.0);
        let t = Transport { playing: true, pos_beats: Some(2.0), tempo: Some(120.0) };
        e.begin_block(&state, &vals, &t);
        e.next_frame(&state, &vals);
        assert_eq!(state.cur_step.load(Relaxed), 8); // beat 2 = step 9
    }

    #[test]
    fn multiply_splits_a_step() {
        let state = SharedState::default();
        for c in 0..NUM_CH {
            for p in 0..16 {
                state.steps[p][c].store(0, Relaxed);
            }
        }
        state.set_step(0, 0, 0, true);
        state.cycle_hits(0, 0, 0); // 2
        state.cycle_hits(0, 0, 0); // 3
        assert_eq!(state.hits_at(0, 0, 0), 3);
        let vals = defaults();
        let mut e = Engine::new(48000.0);
        state.running.store(true, Relaxed);
        e.begin_block(&state, &vals, &Transport::default());
        let before = state.hits[0].load(Relaxed);
        for _ in 0..5400 {
            e.next_frame(&state, &vals);
        }
        assert_eq!(state.hits[0].load(Relaxed) - before, 3);
        state.cycle_hits(0, 0, 0); // 4
        state.cycle_hits(0, 0, 0); // off
        assert_eq!(state.hits_at(0, 0, 0), 0);
    }

    #[test]
    fn program_keys_and_start_keys() {
        let state = SharedState::default();
        let mut e = Engine::new(48000.0);
        e.note_on(&state, 0, PROGRAM_NOTE + 5, 1.0);
        assert_eq!(state.program(), 5);
        e.note_on(&state, 0, START_NOTE, 1.0);
        assert!(state.running.load(Relaxed));
        e.note_off(&state, 0, START_NOTE);
        assert!(!state.running.load(Relaxed));
    }
}
