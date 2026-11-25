//! Sequencer clock, MIDI key map and mixer.

use crate::dsp::{Voice, VoiceSettings};
use crate::fx::{Fx, FxValues};
use crate::params::{FX_PARAMS, NUM_FX_BUSES};
use crate::params::{internal_bpm, internal_steps_per_sec, sync_division, ParamId, NUM_CH, SYNC_STEPS_PER_BEAT, VOICE_PARAMS};
use crate::state::SharedState;

/// A queued step of one voice (NUDGE): samples left, hits, accent (0 = plain).
#[derive(Clone, Copy)]
struct Queued {
    delay: f64,
    hits: u32,
    accent: f32,
}
const NO_QUEUE: Queued = Queued { delay: 0.0, hits: 0, accent: 0.0 };
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
    /// TEMPO DRIFT 0..1 and SOUND DRIFT 0..1.
    pub tempo_drift: f32,
    pub sound_drift: f32,
    /// SWING 0..1: every second step late by up to half a step.
    pub swing: f32,
    /// ACCENT 0..1: strength of accented steps.
    pub accent: f32,
    /// FX sends per voice (0 PHASER, 1 DIST, 2 DELAY, 3 SPRING) and the FX section settings.
    pub sends: [[f32; NUM_FX_BUSES]; NUM_CH],
    pub fx: FxValues,
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
            tempo_drift: get(ParamId::TempoDrift).clamp(0.0, 1.0),
            sound_drift: get(ParamId::SoundDrift).clamp(0.0, 1.0),
            swing: get(ParamId::Swing).clamp(0.0, 1.0),
            accent: get(ParamId::Accent).clamp(0.0, 1.0),
            sends: std::array::from_fn(|c| std::array::from_fn(|k| get(ParamId::Send(c, k)).clamp(0.0, 1.0))),
            fx: std::array::from_fn(|i| get(ParamId::Fx(FX_PARAMS[i]))),
            voices: std::array::from_fn(|c| {
                let arr: [f32; 16] = std::array::from_fn(|i| get(ParamId::Voice(c, VOICE_PARAMS[i])));
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
    fx: Fx,
    /// tempo the FX delay syncs to (host tempo, or the internal clock's BPM)
    fx_bpm: f64,
    voices: [Voice; NUM_CH],
    counter: f64,
    step: usize,
    was_running: bool,
    held_start_keys: u32,
    pan_random: [f32; NUM_CH],
    /// Multiply: hits still to play inside the current step, and samples until the next one.
    pending: [u32; NUM_CH],
    /// accent of the step the MULTIPLY hits belong to
    pending_accent: [f32; NUM_CH],
    countdown: [f64; NUM_CH],
    interval: [f64; NUM_CH],
    /// Length of the current step in samples.
    step_samples: f64,
    /// TEMPO DRIFT. The grid never moves: `t_samples` counts time since START and `grid_next`
    /// is where the next step sits on the grid; the step plays at grid + its own shift.
    /// `abs_steps` is the index of the next step to play (internal clock), `host_next` the same
    /// in host-sync mode. Each 128-step cycle has its own slow wave and in-bar pattern.
    abs_steps: u64,
    t_samples: f64,
    grid_next: f64,
    host_next: i64,
    wave_amp: [f32; 3],
    wave_phase: [f32; 3],
    bar_pattern: [f32; 16],
    jitter_seed: u32,
    rng: u32,
    sound_drift: f32,
    /// SOUND DRIFT share per voice (column B) and the swing of this block.
    drift_share: [f32; NUM_CH],
    swing: f32,
    /// NUDGE (column A): up to two hits per voice waiting to be played (samples left, hits).
    nudged: [[Queued; 2]; NUM_CH],
    /// Step of each voice in its own loop (polymetry: voices may have different lengths).
    voice_step: [usize; NUM_CH],
    /// ACCENT knob of this block
    accent: f32,
    /// true for the first step after START (early voices play it at once)
    first_step: bool,
    /// Pattern chain: the slot playing (None when the chain isn't playing), the slot that
    /// comes next (drawn in advance in RANDOM mode, so an early NUDGE voice can read it),
    /// the step inside the chained pattern (host sync) and whether to find the place in the
    /// chain again from the host position (start, loop or jump).
    chain_slot: Option<usize>,
    chain_next: Option<usize>,
    chain_step: usize,
    chain_sync: bool,
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
            fx: Fx::new(sample_rate.max(8000.0)),
            fx_bpm: 120.0,
            voices: std::array::from_fn(|i| Voice::new(0x9E37_79B9u32.wrapping_mul(i as u32 + 1))),
            counter: 0.0,
            step: 0,
            was_running: false,
            held_start_keys: 0,
            pan_random: [0.0; NUM_CH],
            pending: [0; NUM_CH],
            pending_accent: [0.0; NUM_CH],
            countdown: [0.0; NUM_CH],
            interval: [0.0; NUM_CH],
            step_samples: 0.0,
            abs_steps: 0,
            t_samples: 0.0,
            grid_next: 0.0,
            host_next: i64::MIN,
            wave_amp: [0.0; 3],
            wave_phase: [0.0; 3],
            bar_pattern: [0.0; 16],
            jitter_seed: 0x1234_5678,
            rng: 0x2545_F491,
            sound_drift: 0.0,
            drift_share: [1.0; NUM_CH],
            swing: 0.0,
            nudged: [[NO_QUEUE; 2]; NUM_CH],
            voice_step: [0; NUM_CH],
            accent: 0.5,
            first_step: true,
            chain_slot: None,
            chain_next: None,
            chain_step: 0,
            chain_sync: true,
            host_active: false,
            block_pos0: 0.0,
            beats_per_sample: 0.0,
            host_tempo: 120.0,
            frame_in_block: 0,
        }
    }

    /// Call outside the audio thread (plugin initialize, before the stream starts): it may
    /// allocate the FX buffers for the new rate.
    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr.max(8000.0);
        if (self.fx.sample_rate() - self.sr).abs() > 0.5 {
            self.fx = Fx::new(self.sr);
        }
    }

    pub fn reset(&mut self) {
        self.voices = std::array::from_fn(|i| Voice::new(0x9E37_79B9u32.wrapping_mul(i as u32 + 1)));
        self.counter = 0.0;
        self.host_next = i64::MIN;
        self.held_start_keys = 0;
        self.pending = [0; NUM_CH];
        self.nudged = [[NO_QUEUE; 2]; NUM_CH];
        self.abs_steps = 0;
        self.t_samples = 0.0;
        self.grid_next = 0.0;
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
            self.strike(state, ch, false, 0.0);
        } else if !state.keyb_ctrl.load(Relaxed) {
        } else if note < START_NOTE {
            state.program.store(note - PROGRAM_NOTE, Relaxed);
        } else {
            self.held_start_keys += 1;
            state.running.store(true, Relaxed);
        }
    }

    pub fn note_off(&mut self, state: &SharedState, channel: u8, note: u8) {
        // Releasing a start key always counts, even if KEYB CTRL was switched off meanwhile,
        // so the sequencer can't stay stuck running.
        if !Self::channel_ok(state, channel) || note < START_NOTE {
            return;
        }
        if self.held_start_keys > 0 {
            self.held_start_keys -= 1;
            if self.held_start_keys == 0 {
                state.running.store(false, Relaxed);
            }
        }
    }

    fn strike(&mut self, state: &SharedState, ch: usize, stacked: bool, accent: f32) {
        self.voices[ch].strike(self.pan_random[ch], self.sound_drift * self.drift_share[ch], stacked, accent);
        state.hits[ch].fetch_add(1, Relaxed);
    }

    /// Call once per audio block before pulling frames.
    pub fn begin_block(&mut self, state: &SharedState, vals: &Values, transport: &Transport) {
        let req = state.trig_req.swap(0, Relaxed);
        for ch in 0..NUM_CH {
            if req & (1 << ch) != 0 {
                self.strike(state, ch, false, 0.0);
            }
        }
        for ch in 0..NUM_CH {
            self.pan_random[ch] = vals.voices[ch].pan_random;
            self.drift_share[ch] = vals.voices[ch].drift_amount.clamp(0.0, 1.0);
        }
        self.swing = vals.swing;
        self.accent = vals.accent;
        self.sound_drift = vals.sound_drift;
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
            self.host_next = i64::MIN;
            if !state.running.load(Relaxed) {
                state.cur_step.store(-1, Relaxed);
                for v in state.voice_step.iter() {
                    v.store(-1, Relaxed);
                }
                self.nudged = [[NO_QUEUE; 2]; NUM_CH];
            }
        }
        self.frame_in_block = 0;
        if !state.chain_active() {
            self.chain_slot = None;
            self.chain_next = None;
            state.chain_slot.store(-1, Relaxed);
        }
        state.host_driven.store(self.host_active, Relaxed);
        let bpm = if vals.sync { self.host_tempo as f32 } else { internal_bpm(vals.tempo) };
        state.bpm_bits.store(bpm.to_bits(), Relaxed);
        self.fx_bpm = if self.host_active || vals.sync { self.host_tempo } else { internal_bpm(vals.tempo) as f64 };
        if state.spring_kick.swap(false, Relaxed) {
            self.fx.kick_spring();
        }
    }

    /// A step is reached. Voices on the grid play now; NUDGE (column A) moves a voice up to half
    /// a step: late voices are queued for later in this step, early voices are queued from this
    /// step for the next one (so they land just before it, like disco claps before the snare).
    ///
    /// Each voice reads its own step (`voice_step`), which loops over the voice's own length.
    /// PROB is drawn once per step and voice, when the step is reached (or queued, for an early
    /// voice); a step that loses the draw is silent, MULTIPLY hits included.
    fn fire(&mut self, state: &SharedState, vals: &Values) {
        state.cur_step.store(self.step as i32, Relaxed);
        let prog = state.program();
        let l = self.step_samples;
        // last step of a chained pattern: what comes next is the first step of the next one
        let boundary = self.chain_slot.is_some() && state.chain_active() && self.step + 1 >= state.length(prog);
        let next_prog = if boundary { self.chain_peek_program(state) } else { prog };
        for ch in 0..NUM_CH {
            let vlen = state.voice_length(prog, ch);
            let step = self.voice_step[ch] % vlen;
            state.voice_step[ch].store(step as i8, Relaxed);
            let next = if boundary { 0 } else { (step + 1) % vlen };
            let nudge = vals.voices[ch].nudge.clamp(-1.0, 1.0) as f64;
            if nudge >= 0.0 {
                let (cur, acc) = self.draw_step(state, prog, ch, step);
                if cur > 0 {
                    let d = nudge * 0.5 * l;
                    if d < 0.5 {
                        self.play_step(state, vals, ch, cur, acc);
                    } else {
                        self.queue(ch, d, cur, acc);
                    }
                }
            } else {
                if self.first_step {
                    // nothing before START: an early voice plays its first step at once
                    let (cur, acc) = self.draw_step(state, prog, ch, step);
                    if cur > 0 {
                        self.play_step(state, vals, ch, cur, acc);
                    }
                }
                let (nx, acc) = self.draw_step(state, next_prog, ch, next);
                if nx > 0 {
                    self.queue(ch, (1.0 + nudge * 0.5) * l, nx, acc);
                }
            }
        }
        self.first_step = false;
    }

    /// Hits a voice plays on `step` (0 = silent, PROB draw included) and its accent strength.
    fn draw_step(&mut self, state: &SharedState, prog: usize, ch: usize, step: usize) -> (u32, f32) {
        let hits = state.hits_at(prog, ch, step);
        if hits == 0 {
            return (0, 0.0);
        }
        let chance = state.prob_at(prog, ch, step);
        if chance < 100 && self.rand() * 100.0 >= chance as f32 {
            return (0, 0.0);
        }
        let acc = if state.accent_on(prog, ch, step) { self.accent } else { 0.0 };
        (hits, acc)
    }

    /// Strike a voice for one step (with its MULTIPLY hits spread over the step).
    fn play_step(&mut self, state: &SharedState, vals: &Values, ch: usize, n: u32, accent: f32) {
        let _ = vals;
        // MUTE only takes the voice out of the Mix (see next_frame): it still plays on its own output
        if state.row_audible(ch, false) {
            self.strike(state, ch, false, accent);
            self.pending_accent[ch] = accent;
            // Multiply: the step keeps its length and is split into n equal hits.
            self.pending[ch] = n - 1;
            self.interval[ch] = self.step_samples / n as f64;
            self.countdown[ch] = self.interval[ch];
        } else {
            self.pending[ch] = 0;
        }
    }

    fn queue(&mut self, ch: usize, delay: f64, n: u32, accent: f32) {
        let slot = if self.nudged[ch][0].hits == 0 { 0 } else if self.nudged[ch][1].hits == 0 { 1 } else { 0 };
        self.nudged[ch][slot] = Queued { delay, hits: n, accent };
    }

    /// Advance every voice to the step of absolute step `n` (internal clock: by one, so a length
    /// changed while playing takes effect at once; host sync: from the host position).
    fn advance_voices(&mut self, state: &SharedState, n: i64, from_host: bool) {
        let prog = state.program();
        for ch in 0..NUM_CH {
            let vlen = state.voice_length(prog, ch);
            self.voice_step[ch] = if from_host {
                n.rem_euclid(vlen as i64) as usize
            } else if n == 0 {
                0
            } else {
                (self.voice_step[ch] + 1) % vlen
            };
        }
    }

    /// A random slot index below k.
    fn rand_index(&mut self, k: usize) -> usize {
        ((self.rand() * k as f32) as usize).min(k.saturating_sub(1))
    }

    /// The slot that follows `cur` in the chain: left to right (back to the first after the
    /// last), or in RANDOM mode any chained slot but the one just played.
    fn chain_pick_after(&mut self, state: &SharedState, cur: Option<usize>) -> usize {
        let k = state.chain_len().max(1);
        if state.chain_random.load(Relaxed) && k > 1 {
            loop {
                let r = self.rand_index(k);
                if Some(r) != cur {
                    return r;
                }
            }
        }
        match cur {
            Some(c) if c + 1 < k => c + 1,
            _ => 0,
        }
    }

    /// Play chain slot `slot` now: its program becomes the current one, the next slot is drawn.
    fn chain_go(&mut self, state: &SharedState, slot: usize) {
        self.chain_slot = Some(slot);
        state.chain_slot.store(slot as i8, Relaxed);
        if let Some(p) = state.chain_program(slot) {
            state.program.store(p as u8, Relaxed);
        }
        self.chain_next = Some(self.chain_pick_after(state, Some(slot)));
    }

    /// The next slot (still valid if the chain was edited meanwhile).
    fn chain_peek(&mut self, state: &SharedState) -> usize {
        let k = state.chain_len();
        match self.chain_next {
            Some(n) if n < k => n,
            _ => {
                let cur = self.chain_slot.filter(|&c| c < k);
                let n = self.chain_pick_after(state, cur);
                self.chain_next = Some(n);
                n
            }
        }
    }

    fn chain_peek_program(&mut self, state: &SharedState) -> usize {
        let slot = self.chain_peek(state);
        state.chain_program(slot).unwrap_or_else(|| state.program())
    }

    /// START with the chain on: the first slot (a random one in RANDOM mode).
    fn chain_start(&mut self, state: &SharedState) {
        let first = self.chain_pick_after(state, None);
        let first = if state.chain_random.load(Relaxed) { first } else { 0 };
        self.chain_go(state, first);
    }

    /// End of a chained pattern: on to the next slot.
    fn chain_advance(&mut self, state: &SharedState) {
        let next = self.chain_peek(state);
        self.chain_go(state, next);
    }

    /// Host sync: find the slot and the step inside it for absolute host step `n`, so the
    /// chain stays in place after a loop or a jump in the song (RANDOM: a random slot).
    fn chain_locate(&mut self, state: &SharedState, n: i64) {
        let k = state.chain_len();
        if state.chain_random.load(Relaxed) {
            let slot = self.rand_index(k);
            self.chain_go(state, slot);
            self.chain_step = n.rem_euclid(state.length(state.program()) as i64) as usize;
            return;
        }
        let lens: Vec<i64> = (0..k).map(|i| state.length(state.chain_program(i).unwrap_or(0)) as i64).collect();
        let total: i64 = lens.iter().sum::<i64>().max(1);
        let mut r = n.rem_euclid(total);
        for (i, l) in lens.iter().enumerate() {
            if r < *l {
                self.chain_go(state, i);
                self.chain_step = r as usize;
                return;
            }
            r -= l;
        }
        self.chain_go(state, 0);
        self.chain_step = 0;
    }

    fn clock(&mut self, state: &SharedState, vals: &Values) {
        let prog = state.program();
        let len = state.length(prog);
        if self.host_active {
            let spq = SYNC_STEPS_PER_BEAT[sync_division(vals.tempo)];
            self.step_samples = self.sr as f64 * 60.0 / (self.host_tempo * spq);
            let beat = self.block_pos0 + self.frame_in_block as f64 * self.beats_per_sample;
            // host position in steps; step k plays at k + its shift (in steps)
            let pos = beat * spq + 1e-9;
            if self.host_next == i64::MIN
                || pos < self.host_next as f64 - 1.5
                || pos > self.host_next as f64 + 1.5
            {
                // start, loop or jump: next step is the first one not yet passed
                self.first_step = true;
                self.nudged = [[NO_QUEUE; 2]; NUM_CH];
                self.chain_sync = true;
                self.host_next = pos.ceil() as i64;
                if pos.fract() < 1e-6 {
                    self.host_next = pos.floor() as i64;
                }
            }
            while pos >= self.host_next as f64 + self.step_shift(self.host_next, vals.tempo_drift) {
                let n = self.host_next;
                if n.rem_euclid(128) == 0 {
                    self.new_drift_cycle(n);
                }
                if state.chain_active() {
                    if self.chain_sync || self.chain_slot.is_none() {
                        self.chain_locate(state, n);
                        self.chain_sync = false;
                    } else if self.chain_step >= state.length(state.program()) {
                        // the pattern has ended: the next chained one starts on this step
                        self.chain_advance(state);
                        self.chain_step = 0;
                    }
                    self.step = self.chain_step;
                    // each voice from the start of the chained pattern
                    let prog = state.program();
                    for ch in 0..NUM_CH {
                        self.voice_step[ch] = self.chain_step % state.voice_length(prog, ch);
                    }
                } else {
                    self.chain_sync = true;
                    self.step = n.rem_euclid(len as i64) as usize;
                    self.advance_voices(state, n, true);
                }
                self.host_next += 1;
                self.fire(state, vals);
                if self.chain_slot.is_some() && state.chain_active() {
                    self.chain_step += 1;
                }
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
                // START: the grid starts now, step 0 sits exactly on it
                self.first_step = true;
                self.nudged = [[NO_QUEUE; 2]; NUM_CH];
                self.t_samples = 0.0;
                self.grid_next = 0.0;
                self.abs_steps = 0;
            } else {
                self.t_samples += 1.0;
            }
            // play every step whose (grid position + shift) has been reached; the grid itself
            // keeps the exact tempo, so nothing ever has to catch up
            loop {
                let n = self.abs_steps as i64;
                let at = self.grid_next + self.step_shift(n, vals.tempo_drift) * samples_per_step;
                if self.t_samples < at {
                    break;
                }
                if n.rem_euclid(128) == 0 {
                    self.new_drift_cycle(n);
                }
                let mut restart = false;
                if n == 0 {
                    if state.chain_active() {
                        self.chain_start(state);
                    }
                    self.step = 0;
                } else if self.step + 1 >= state.length(state.program()) {
                    // end of the pattern: with the chain on, the next chained one starts
                    if state.chain_active() {
                        self.chain_advance(state);
                        restart = true;
                    }
                    self.step = 0;
                } else {
                    self.step += 1;
                }
                if restart {
                    self.voice_step = [0; NUM_CH];
                } else {
                    self.advance_voices(state, n, false);
                }
                self.abs_steps += 1;
                self.grid_next += samples_per_step;
                self.fire(state, vals);
            }
        } else if self.was_running {
            state.cur_step.store(-1, Relaxed);
            for v in state.voice_step.iter() {
                v.store(-1, Relaxed);
            }
            self.pending = [0; NUM_CH];
            self.nudged = [[NO_QUEUE; 2]; NUM_CH];
        }
        self.was_running = running;
    }

    pub fn next_frame(&mut self, state: &SharedState, vals: &Values) -> Frame {
        self.clock(state, vals);
        self.frame_in_block += 1;
        for ch in 0..NUM_CH {
            for q in 0..2 {
                let Queued { delay, hits, accent } = self.nudged[ch][q];
                if hits > 0 {
                    if delay <= 1.0 {
                        self.nudged[ch][q] = NO_QUEUE;
                        self.play_step(state, vals, ch, hits, accent);
                    } else {
                        self.nudged[ch][q].delay = delay - 1.0;
                    }
                }
            }
        }
        for ch in 0..NUM_CH {
            if self.pending[ch] > 0 {
                self.countdown[ch] -= 1.0;
                if self.countdown[ch] <= 0.0 {
                    self.countdown[ch] += self.interval[ch];
                    self.pending[ch] -= 1;
                    if state.row_audible(ch, false) {
                        let a = self.pending_accent[ch];
                        self.strike(state, ch, true, a);
                    }
                }
            }
        }
        let mut f = Frame::default();
        let mut bus = [[0.0f32; 2]; NUM_FX_BUSES];
        for ch in 0..NUM_CH {
            let raw = self.voices[ch].render(&vals.voices[ch], self.sr, vals.classic);
            // Level Adj: linear, 10 V = unity.
            let gain = vals.level[ch] / 10.0;
            let v = raw * gain;
            f.voices[ch] = v;
            // Pan -5..+5 V (+ Pan Random offset), equal-power law (estimated).
            let pan = ((vals.pan[ch] + self.voices[ch].pan_offset) / 5.0).clamp(-1.0, 1.0);
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            let (l, r) = (v * angle.cos(), v * angle.sin());
            // MUTE takes the voice (and its FX sends) out of the stereo Mix only: its own
            // output keeps playing, so a voice sent to its own track is never doubled in the Mix
            if vals.mute[ch] {
                continue;
            }
            f.left += l;
            f.right += r;
            // post-fader, post-pan sends to the shared FX buses
            for k in 0..NUM_FX_BUSES {
                let s = vals.sends[ch][k];
                if s > 0.0 {
                    bus[k][0] += l * s;
                    bus[k][1] += r * s;
                }
            }
        }
        // FX returns join the stereo mix only; the six individual outputs stay dry
        let ret = self.fx.process(&bus, &vals.fx, self.fx_bpm);
        f.left += ret[0];
        f.right += ret[1];
        let m = vals.master / 10.0;
        f.left = (f.left * m).clamp(-4.0, 4.0);
        f.right = (f.right * m).clamp(-4.0, 4.0);
        // never let an invalid value reach the host
        if !f.left.is_finite() || !f.right.is_finite() {
            f = Frame::default();
            self.voices = std::array::from_fn(|i| Voice::new(0x9E37_79B9u32.wrapping_mul(i as u32 + 1)));
        }
        f
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x as f32 / u32::MAX as f32
    }

    /// New character for the next 8 bars (128 steps): a slow wave (1, 2 and 3 cycles per 128
    /// steps), an in-bar pattern (which of the 16 positions lean early or late) and a seed for
    /// the per-step looseness.
    fn new_drift_cycle(&mut self, n: i64) {
        let a: [f32; 3] = [0.6 + 0.4 * self.rand(), 0.5 * self.rand(), 0.3 * self.rand()];
        let total: f32 = a.iter().sum::<f32>().max(1e-3);
        for k in 0..3 {
            self.wave_amp[k] = a[k] / total;
            self.wave_phase[k] = self.rand() * std::f32::consts::TAU;
        }
        let mut mean = 0.0;
        for i in 0..16 {
            let v = self.rng_signed();
            self.bar_pattern[i] = v;
            mean += v / 16.0;
        }
        for p in self.bar_pattern.iter_mut() {
            *p = (*p - mean).clamp(-1.0, 1.0);
        }
        self.jitter_seed = (n as u32).wrapping_mul(0x9E37_79B9) ^ self.rng;
    }

    fn rng_signed(&mut self) -> f32 {
        self.rand() * 2.0 - 1.0
    }

    /// Drift wave at step m (0..128) of the cycle, in -1..1.
    fn drift_wave(&self, m: i64) -> f32 {
        let x = m.rem_euclid(128) as f32 / 128.0 * std::f32::consts::TAU;
        (0..3).map(|k| self.wave_amp[k] * ((k as f32 + 1.0) * x + self.wave_phase[k]).sin()).sum()
    }

    /// TEMPO DRIFT: how early (-) or late (+) step `n` plays, in steps, around its fixed grid
    /// position. At 100 % it is up to about +-0.3 step: in-bar pattern (12 %), slow wave (15 %)
    /// and looseness (4 %), all faded by a window that is 0 at every 8-bar boundary, so steps
    /// slide around inside the bars without the tempo speeding up, slowing down or jumping.
    ///
    /// The knob is two-sided: 0.5 = no drift; below 0.5 every step can only fall late (up to
    /// 0.4 step at 0 %), above 0.5 only early (up to 0.4 step at 100 %). The random structure
    /// below sets how far each step goes, from not at all to the maximum.
    fn step_shift(&self, n: i64, knob: f32) -> f64 {
        // SWING: every second step (the off-beat 16ths) late by up to half a step
        let swing = if n.rem_euclid(2) == 1 { (self.swing.clamp(0.0, 1.0) * 0.5) as f64 } else { 0.0 };
        swing + self.drift_shift(n, knob)
    }

    fn drift_shift(&self, n: i64, knob: f32) -> f64 {
        // + = late, - = early
        let dir = (0.5 - knob.clamp(0.0, 1.0)) * 2.0;
        if dir.abs() < 1e-3 {
            return 0.0;
        }
        let m = n.rem_euclid(128);
        let window = (std::f32::consts::PI * m as f32 / 128.0).sin();
        let mut h = (n as u32).wrapping_mul(0x85EB_CA6B) ^ self.jitter_seed;
        h ^= h >> 13;
        h = h.wrapping_mul(0xC2B2_AE35);
        h ^= h >> 16;
        let jitter = h as f32 / u32::MAX as f32 * 2.0 - 1.0;
        let s = 0.12 * self.bar_pattern[(m % 16) as usize] + 0.15 * self.drift_wave(m) + 0.04 * jitter;
        let magnitude = (s.abs() / 0.31).clamp(0.0, 1.0);
        (dir * window * 0.4 * magnitude) as f64
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
    fn mute_leaves_the_mix_but_keeps_the_voice_output() {
        let state = SharedState::default();
        let mut vals = defaults();
        vals.mute[0] = true;
        vals.sends[0] = [1.0; NUM_FX_BUSES]; // even its FX sends must stay out of the Mix
        let mut e = Engine::new(48000.0);
        e.note_on(&state, 0, BASE_NOTE, 1.0);
        e.begin_block(&state, &vals, &Transport::default());
        let (mut mix, mut own) = (0.0f32, 0.0f32);
        for _ in 0..24000 {
            let f = e.next_frame(&state, &vals);
            mix = mix.max(f.left.abs()).max(f.right.abs());
            own = own.max(f.voices[0].abs());
        }
        assert!(mix < 1e-6, "a muted voice reached the Mix: {mix}");
        assert!(own > 0.02, "the muted voice should still play on its own output: {own}");
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

    /// Step start times (samples) for the first 258 steps with the given TEMPO DRIFT knob.
    fn drift_run(knob: f32) -> (Vec<f64>, f64) {
        let state = SharedState::default();
        let mut vals = defaults();
        vals.tempo = 6.1;
        vals.tempo_drift = knob;
        let mut e = Engine::new(48000.0);
        state.running.store(true, Relaxed);
        e.begin_block(&state, &vals, &Transport::default());
        let nominal = 48000.0 / internal_steps_per_sec(6.1) as f64;
        let mut starts = Vec::new();
        let mut last = -1i32;
        for i in 0..(nominal * 257.5) as u64 {
            e.next_frame(&state, &vals);
            let cur = state.cur_step.load(Relaxed);
            if cur != last {
                last = cur;
                starts.push(i as f64);
            }
        }
        (starts, nominal)
    }

    #[test]
    fn tempo_drift_knob_is_two_sided() {
        // 50 %: exactly on the grid
        let (s, nom) = drift_run(0.5);
        assert!(s.iter().enumerate().all(|(k, t)| (t - k as f64 * nom).abs() <= 1.0));
        // 0 %: steps only fall late (behind the beat), some by more than 10 % of a step
        let (s, nom) = drift_run(0.0);
        assert!(s.iter().enumerate().all(|(k, t)| *t >= k as f64 * nom - 1.0));
        assert!(s.iter().enumerate().any(|(k, t)| t - k as f64 * nom > nom * 0.1));
        // 100 %: steps only fall early (ahead of the beat)
        let (s, nom) = drift_run(1.0);
        assert!(s.iter().enumerate().all(|(k, t)| *t <= k as f64 * nom + 1.0));
        assert!(s.iter().enumerate().any(|(k, t)| k as f64 * nom - t > nom * 0.1));
    }

    #[test]
    fn tempo_drift_realigns_every_128_steps() {
        let (starts, nominal) = drift_run(1.0);
        let moved = starts.iter().enumerate().any(|(k, t)| (t - k as f64 * nominal).abs() > nominal * 0.05);
        assert!(moved, "drift should move steps off the grid");
        // steps only slide around the grid: every gap stays between 0.4 and 1.6 steps, and the
        // gaps around each 8-bar boundary are close to normal (no catch-up jolt)
        for w in starts.windows(2) {
            let gap = (w[1] - w[0]) / nominal;
            assert!(gap > 0.4 && gap < 1.6, "gap {gap}");
        }
        for b in [128usize, 256] {
            for k in b - 1..=b {
                let gap = (starts[k + 1] - starts[k]) / nominal;
                assert!((gap - 1.0).abs() < 0.1, "gap at {k}: {gap}");
            }
        }
        // step 128 and step 256 land back on the grid (within 2 samples)
        assert!((starts[128] - 128.0 * nominal).abs() < 2.0, "{}", starts[128] - 128.0 * nominal);
        assert!((starts[256] - 256.0 * nominal).abs() < 3.0, "{}", starts[256] - 256.0 * nominal);
    }

    #[test]
    fn host_sync_drift_keeps_every_step_once() {
        let state = SharedState::default();
        let mut vals = defaults();
        vals.sync = true;
        vals.tempo = 6.0; // 1/16
        vals.tempo_drift = 1.0;
        let mut e = Engine::new(48000.0);
        let block = 256usize;
        let bps = 120.0 / 60.0 / 48000.0;
        let mut fires = 0;
        let mut last = -1;
        let total_steps = 300.0;
        let frames = (total_steps / 4.0 / bps) as usize;
        let mut pos = 0.0f64;
        let mut done = 0;
        while done < frames {
            let t = Transport { playing: true, pos_beats: Some(pos), tempo: Some(120.0) };
            e.begin_block(&state, &vals, &t);
            for _ in 0..block {
                e.next_frame(&state, &vals);
                let cur = state.cur_step.load(Relaxed);
                if cur != last {
                    last = cur;
                    fires += 1;
                }
            }
            pos += block as f64 * bps;
            done += block;
        }
        let expected = (done as f64 * bps * 4.0) as i64;
        assert!((fires as i64 - expected).abs() <= 1, "fires {fires} expected {expected}");
    }

    /// Sample at which voice `ch` is first struck after START, with the given settings.
    fn first_hits(mut vals: Values, ch: usize, steps: usize) -> Vec<u64> {
        let state = SharedState::default();
        for p in 0..16 {
            for c in 0..NUM_CH {
                state.steps[p][c].store(0, Relaxed);
            }
        }
        state.set_step(0, ch, 4, true); // step 5
        vals.tempo = 6.1;
        let mut e = Engine::new(48000.0);
        state.running.store(true, Relaxed);
        e.begin_block(&state, &vals, &Transport::default());
        let nominal = 48000.0 / internal_steps_per_sec(6.1) as f64;
        let mut hits = Vec::new();
        let mut seen = state.hits[ch].load(Relaxed);
        for i in 0..(nominal * steps as f64) as u64 {
            e.next_frame(&state, &vals);
            let h = state.hits[ch].load(Relaxed);
            if h != seen {
                seen = h;
                hits.push(i);
            }
        }
        hits
    }

    #[test]
    fn nudge_moves_a_voice_early_or_late() {
        let nominal = 48000.0 / internal_steps_per_sec(6.1) as f64;
        let mut v = defaults();
        let on_grid = first_hits(v, 1, 8)[0] as f64;
        assert!((on_grid - 4.0 * nominal).abs() < 2.0);
        v.voices[1].nudge = -0.5; // a quarter of a step early (disco clap)
        let early = first_hits(v, 1, 8)[0] as f64;
        assert!((early - (4.0 - 0.25) * nominal).abs() < 2.0, "early {early}");
        v.voices[1].nudge = 1.0; // half a step late
        let late = first_hits(v, 1, 8)[0] as f64;
        assert!((late - 4.5 * nominal).abs() < 2.0, "late {late}");
    }

    #[test]
    fn swing_delays_every_second_step() {
        let nominal = 48000.0 / internal_steps_per_sec(6.1) as f64;
        let mut v = defaults();
        v.swing = 1.0;
        let state = SharedState::default();
        v.tempo = 6.1;
        let mut e = Engine::new(48000.0);
        state.running.store(true, Relaxed);
        e.begin_block(&state, &v, &Transport::default());
        let mut starts = Vec::new();
        let mut last = -1;
        for i in 0..(nominal * 4.2) as u64 {
            e.next_frame(&state, &v);
            let c = state.cur_step.load(Relaxed);
            if c != last {
                last = c;
                starts.push(i as f64);
            }
        }
        assert!((starts[1] - 1.5 * nominal).abs() < 2.0, "{}", starts[1] / nominal);
        assert!((starts[2] - 2.0 * nominal).abs() < 2.0);
        assert!((starts[3] - 3.5 * nominal).abs() < 2.0);
    }

    #[test]
    fn multiply_hits_saturate() {
        // the overload grows gradually as hits come closer together: peak of the last hit of a
        // roll, relative to a lone hit, for hits 1 s, 250 ms, 125 ms, 60 ms, 30 ms, 15 ms apart
        let vals = defaults();
        let s = vals.voices[0];
        let sr = 48000.0;
        let last_peak = |gap: f32, hits: usize| {
            let mut v = Voice::new(7);
            let step = (gap * sr) as usize;
            let mut peak = 0.0f32;
            for i in 0..(step * (hits - 1) + 4800) {
                if i % step == 0 && i / step < hits {
                    v.strike(0.0, 0.0, false, 0.0);
                }
                let y = v.render(&s, sr, true);
                assert!(y.is_finite() && y.abs() <= 2.0);
                if i >= step * (hits - 1) {
                    peak = peak.max(y.abs());
                }
            }
            peak
        };
        let lone = last_peak(1.0, 1);
        let gaps = [1.0f32, 0.25, 0.125, 0.06, 0.03, 0.015];
        let peaks: Vec<f32> = gaps.iter().map(|g| last_peak(*g, 4) / lone).collect();
        assert!((peaks[0] - 1.0).abs() < 0.01, "hits 1 s apart must stay clean: {peaks:?}");
        for w in peaks.windows(2) {
            assert!(w[1] >= w[0] - 1e-4, "the overload must grow as hits get closer: {peaks:?}");
        }
        assert!(peaks[5] > peaks[0] * 1.05, "a fast roll should overload: {peaks:?}");
        // 2 hits in a step overload less than 4
        assert!(last_peak(0.03, 2) <= last_peak(0.03, 4) + 1e-4);
        eprintln!("retrigger overload (4th hit / lone hit) for gaps {gaps:?}: {peaks:?}");
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

    fn empty_state() -> SharedState {
        let state = SharedState::default();
        for p in 0..16 {
            state.clear_program(p);
        }
        state
    }

    /// Hits per voice over `steps` steps of the internal clock.
    fn count_hits(state: &SharedState, vals: &Values, steps: f64) -> [u32; NUM_CH] {
        let mut e = Engine::new(48000.0);
        state.running.store(true, Relaxed);
        e.begin_block(state, vals, &Transport::default());
        let before: [u32; NUM_CH] = std::array::from_fn(|c| state.hits[c].load(Relaxed));
        let nominal = 48000.0 / internal_steps_per_sec(vals.tempo) as f64;
        for _ in 0..(nominal * steps) as u64 {
            e.next_frame(state, vals);
        }
        std::array::from_fn(|c| state.hits[c].load(Relaxed) - before[c])
    }

    #[test]
    fn each_voice_loops_over_its_own_length() {
        let state = empty_state();
        state.set_step(0, 0, 0, true);
        state.set_step(0, 1, 0, true);
        state.set_voice_length(0, 0, 3); // A: 3-step loop
        state.set_voice_length(0, 2, 5); // C: 5 steps (no notes): the program follows the longest
        assert_eq!(state.voice_length(0, 1), 16);
        assert_eq!(state.length(0), 16);
        let mut vals = defaults();
        vals.tempo = 6.1;
        // 48 steps: A plays on steps 0, 3, 6 .. 45 (16 times), B on 0, 16, 32 (3 times)
        let h = count_hits(&state, &vals, 47.5);
        assert_eq!(h[0], 16, "A");
        assert_eq!(h[1], 3, "B");
        // STEPS row: every voice follows the program length again
        state.set_length(0, 8);
        assert!((0..NUM_CH).all(|c| state.voice_length(0, c) == 8));
    }

    #[test]
    fn host_sync_places_each_voice_in_its_own_loop() {
        let state = empty_state();
        state.set_voice_length(0, 0, 3);
        let mut vals = defaults();
        vals.sync = true;
        vals.tempo = 6.0; // 1/16
        let mut e = Engine::new(48000.0);
        let t = Transport { playing: true, pos_beats: Some(2.5), tempo: Some(120.0) };
        e.begin_block(&state, &vals, &t);
        e.next_frame(&state, &vals);
        // step 10 of the song: 10 mod 3 = 1 for A, 10 for B
        assert_eq!(state.voice_step[0].load(Relaxed), 1);
        assert_eq!(state.voice_step[1].load(Relaxed), 10);
    }

    #[test]
    fn probability_thins_out_a_step() {
        let state = empty_state();
        for s in 0..16 {
            state.set_step(0, 0, s, true); // A: always
            state.set_step(0, 1, s, true); // B: 50 %
            state.cycle_prob(0, 1, s);
            state.cycle_prob(0, 1, s);
            assert_eq!(state.prob_at(0, 1, s), 50);
            state.set_step(0, 2, s, true); // C: 25 %
            for _ in 0..3 {
                state.cycle_prob(0, 2, s);
            }
        }
        let mut vals = defaults();
        vals.tempo = 6.1;
        let h = count_hits(&state, &vals, 799.5);
        assert_eq!(h[0], 800);
        assert!((330..=470).contains(&h[1]), "50 %: {}", h[1]);
        assert!((140..=260).contains(&h[2]), "25 %: {}", h[2]);
        // a cleared step forgets its chance
        state.set_step(0, 1, 0, false);
        state.set_step(0, 1, 0, true);
        assert_eq!(state.prob_at(0, 1, 0), 100);
    }

    #[test]
    fn accent_plays_louder() {
        let vals = defaults();
        let s = vals.voices[0];
        let peak = |accent: f32| {
            let mut v = Voice::new(7);
            v.strike(0.0, 0.0, false, accent);
            (0..9600).map(|_| v.render(&s, 48000.0, true).abs()).fold(0.0f32, f32::max)
        };
        let plain = peak(0.0);
        let half = peak(0.5);
        let full = peak(1.0);
        assert!(half > plain * 1.15, "accent 50 %: {half} vs {plain}");
        assert!(full > half, "accent 100 %: {full} vs {half}");
        assert!(full <= 2.0);
        // toggling: an off step comes on accented, a second click removes the accent
        let state = empty_state();
        state.toggle_accent(0, 3, 4);
        assert!(state.step_on(0, 3, 4) && state.accent_on(0, 3, 4));
        state.toggle_accent(0, 3, 4);
        assert!(state.step_on(0, 3, 4) && !state.accent_on(0, 3, 4));
    }

    /// Every step played over `steps` steps of the internal clock: (chain slot, program, step).
    fn chain_trace(state: &SharedState, vals: &Values, steps: f64) -> Vec<(i8, usize, i32)> {
        let mut e = Engine::new(48000.0);
        state.running.store(true, Relaxed);
        e.begin_block(state, vals, &Transport::default());
        let nominal = 48000.0 / internal_steps_per_sec(vals.tempo) as f64;
        let mut seen = Vec::new();
        let mut hits = 0u32;
        let total = |s: &SharedState| (0..NUM_CH).map(|c| s.hits[c].load(Relaxed)).sum::<u32>();
        for _ in 0..(nominal * steps) as u64 {
            let before = state.cur_step.load(Relaxed);
            let prog_before = state.program();
            e.next_frame(state, vals);
            let now = state.cur_step.load(Relaxed);
            if now != before || state.program() != prog_before || total(state) != hits {
                hits = total(state);
                let entry = (state.chain_slot.load(Relaxed), state.program(), now);
                if seen.last() != Some(&entry) {
                    seen.push(entry);
                }
            }
        }
        seen
    }

    fn chain_state() -> SharedState {
        let state = empty_state();
        // program 1: 4 steps, A on 1 · program 2: 8 steps, B on 1 · program 3: 4 steps, C on 1
        // program 5: D on 1, placed after an empty slot (so never played)
        for (prog, len, row) in [(0usize, 4usize, 0usize), (1, 8, 1), (2, 4, 2), (4, 16, 3)] {
            state.set_length(prog, len);
            state.set_step(prog, row, 0, true);
        }
        for (slot, prog) in [(0usize, 1u8), (1, 2), (2, 3), (4, 5)] {
            state.chain[slot].store(prog, Relaxed);
        }
        state
    }

    #[test]
    fn chain_plays_its_patterns_in_order_and_an_empty_slot_starts_it_again() {
        let state = chain_state();
        assert_eq!(state.chain_len(), 3);
        assert!(!state.chain_active(), "off until CHAIN ON");
        state.chain_on.store(true, Relaxed);
        let mut vals = defaults();
        vals.tempo = 6.1;
        // two passes: 2 x (4 + 8 + 4) = 32 steps
        let h = count_hits(&state, &vals, 31.5);
        assert_eq!(h[0], 2, "A (program 1)");
        assert_eq!(h[1], 2, "B (program 2)");
        assert_eq!(h[2], 2, "C (program 3)");
        assert_eq!(h[3], 0, "D (program 5, after the empty slot)");
        // the order and where each pattern starts
        state.running.store(false, Relaxed);
        let trace = chain_trace(&state, &vals, 31.5);
        let starts: Vec<(i8, usize)> = trace.iter().filter(|t| t.2 == 0).map(|t| (t.0, t.1)).collect();
        assert_eq!(starts, vec![(0, 0), (1, 1), (2, 2), (0, 0), (1, 1), (2, 2)]);
        // every pattern plays to its own length
        let longest = trace.iter().filter(|t| t.1 == 1).map(|t| t.2).max().unwrap();
        assert_eq!(longest, 7);
    }

    #[test]
    fn chain_off_keeps_one_pattern() {
        let state = chain_state();
        let mut vals = defaults();
        vals.tempo = 6.1;
        let h = count_hits(&state, &vals, 31.5);
        assert_eq!(h, [8, 0, 0, 0, 0, 0], "program 1 (4 steps) looping");
        assert_eq!(state.chain_slot.load(Relaxed), -1);
    }

    #[test]
    fn random_chain_mixes_the_chosen_patterns_without_playing_one_twice_in_a_row() {
        let state = chain_state();
        state.chain_on.store(true, Relaxed);
        state.chain_random.store(true, Relaxed);
        let mut vals = defaults();
        vals.tempo = 6.1;
        let trace = chain_trace(&state, &vals, 400.0);
        let order: Vec<i8> = trace.iter().filter(|t| t.2 == 0).map(|t| t.0).collect();
        assert!(order.len() > 30, "{} patterns", order.len());
        assert!(order.windows(2).all(|w| w[0] != w[1]), "a pattern twice in a row: {order:?}");
        for slot in 0..3 {
            assert!(order.contains(&slot), "slot {slot} never played");
        }
        assert!(!order.contains(&4), "slot 5 is after the empty slot");
        assert_ne!(order[..12], [0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2], "not random");
    }

    #[test]
    fn host_sync_finds_its_place_in_the_chain() {
        let state = chain_state();
        state.chain[2].store(0, Relaxed); // chain: program 1 (4 steps), program 2 (8 steps)
        state.chain_on.store(true, Relaxed);
        let mut vals = defaults();
        vals.sync = true;
        vals.tempo = 6.0; // 1/16: 4 steps per beat
        for (beats, slot, prog, step) in [(3.5, 0i8, 0usize, 2i32), (5.0, 1, 1, 4), (0.0, 0, 0, 0), (2.75, 1, 1, 7)] {
            let mut e = Engine::new(48000.0);
            let t = Transport { playing: true, pos_beats: Some(beats), tempo: Some(120.0) };
            e.begin_block(&state, &vals, &t);
            e.next_frame(&state, &vals);
            // song step = beats x 4; the chain is 12 steps long
            assert_eq!(state.chain_slot.load(Relaxed), slot, "at beat {beats}");
            assert_eq!(state.program(), prog, "at beat {beats}");
            assert_eq!(state.cur_step.load(Relaxed), step, "at beat {beats}");
        }
    }

    #[test]
    fn chain_is_kept_in_files_and_old_files_have_none() {
        let state = chain_state();
        state.chain_on.store(true, Relaxed);
        state.chain_random.store(true, Relaxed);
        let d = state.snapshot();
        let copy = SharedState::default();
        copy.restore(&serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap());
        assert_eq!(copy.chain_len(), 3);
        assert_eq!(copy.chain_program(4), Some(4));
        assert!(copy.chain_on.load(Relaxed) && copy.chain_random.load(Relaxed));
        let old: crate::state::PatternData = serde_json::from_str(
            r#"{"steps":[[1,0,0,0,0,0]],"lengths":[16],"program":0,"midi_channel":0,"keyb_ctrl":true}"#,
        )
        .unwrap();
        copy.restore(&old);
        assert_eq!(copy.chain_len(), 0);
        assert!(!copy.chain_active());
    }
}
