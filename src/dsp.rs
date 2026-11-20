//! One drum voice, built like the original's sound module:
//! a sine tone oscillator (pitch envelope + phase modulation from a second sine),
//! and white noise through a state-variable filter, each with its own envelope.
//! Conversions follow the original engine (see MECHANICS.md for addresses).

use crate::classic::Classic;
use crate::params::{env_seconds, volts_to_hz, FILTER_HP_THRESHOLD};

/// The original's knob voltages for one voice.
#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceSettings {
    pub tone_tune: f32,
    pub tone_decay: f32,
    pub tone_volume: f32,
    pub pitch_decay: f32,
    pub pitch_decay_vol: f32,
    pub pm_rate: f32,
    pub pm_level: f32,
    pub noise_volume: f32,
    pub noise_decay: f32,
    pub filter_freq: f32,
    pub filter_reso: f32,
    pub filter_decay: f32,
    pub filter_hp: f32,
    pub pan_random: f32,
}

impl VoiceSettings {
    pub fn from_array(v: &[f32; 14]) -> Self {
        VoiceSettings {
            tone_tune: v[0],
            tone_decay: v[1],
            tone_volume: v[2],
            pitch_decay: v[3],
            pitch_decay_vol: v[4],
            pm_rate: v[5],
            pm_level: v[6],
            noise_volume: v[7],
            noise_decay: v[8],
            filter_freq: v[9],
            filter_reso: v[10],
            filter_decay: v[11],
            filter_hp: v[12],
            pan_random: v[13],
        }
    }
}

/// Envelope attack in volts (fixed pin value "-2" on every voice ADSR in the patch).
pub const ATTACK_VOLTS: f32 = -2.0;
/// Oscillator output swing: +-5 V, i.e. +-0.5 at the output (SynthEdit 10 V = 1.0).
pub const OSC_AMPLITUDE: f32 = 0.5;
/// Filter-envelope depth added to the cutoff, in volts (estimated: the patch link wasn't traced).
pub const FILTER_ENV_DEPTH_VOLTS: f32 = 3.0;
/// Highest resonance the filter accepts (Ocet.dll 0x1014fdb8 = 0.9992).
pub const MAX_RESONANCE: f32 = 0.9992;

/// Linear attack / decay envelope (sustain 0), as the original ADSR: each stage moves at
/// 1 / (2^(V - 20/3) * sample rate) per sample (Ocet.dll 0x10080ff0 / 0x10081330).
#[derive(Clone, Copy, Default)]
struct Env {
    level: f32,
    attacking: bool,
}

impl Env {
    fn trigger(&mut self) {
        self.attacking = true;
    }

    fn next(&mut self, attack_step: f32, decay_volts: f32, sr: f32) -> f32 {
        if self.attacking {
            self.level += attack_step;
            if self.level >= 1.0 {
                self.level = 1.0;
                self.attacking = false;
            }
        } else if self.level > 0.0 {
            self.level -= 1.0 / (env_seconds(decay_volts) * sr);
            if self.level < 0.0 {
                self.level = 0.0;
            }
        }
        self.level
    }

    fn idle(&self) -> bool {
        !self.attacking && self.level <= 0.0
    }
}

/// Chamberlin state-variable filter, as the original SV Filter (Ocet.dll 0x10089460):
/// f = 2*pi*fc/sr (fc capped at sr/2), damping q = 2 - 2*resonance.
#[derive(Clone, Copy, Default)]
struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    fn process(&mut self, x: f32, f: f32, q: f32) -> (f32, f32) {
        self.low += f * self.band;
        let high = x - q * self.band - self.low;
        self.band += f * high;
        (self.low, high)
    }
}

pub struct Voice {
    tone_phase: f32,
    pm_phase: f32,
    tone_env: Env,
    pitch_env: Env,
    noise_env: Env,
    filter_env: Env,
    svf: Svf,
    rng: u32,
    /// Pan offset in volts drawn at the last hit (Pan Random).
    pub pan_offset: f32,
    classic: Classic,
}

impl Voice {
    pub fn new(seed: u32) -> Self {
        Voice {
            tone_phase: 0.0,
            pm_phase: 0.0,
            tone_env: Env::default(),
            pitch_env: Env::default(),
            noise_env: Env::default(),
            filter_env: Env::default(),
            svf: Svf::default(),
            rng: seed | 1,
            pan_offset: 0.0,
            classic: Classic::default(),
        }
    }

    #[inline]
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Strike the voice. `pan_random` 0..1 widens a random pan offset (up to +-5 V).
    pub fn trigger(&mut self, pan_random: f32) {
        self.tone_env.trigger();
        self.pitch_env.trigger();
        self.noise_env.trigger();
        self.filter_env.trigger();
        self.classic.trigger();
        self.pan_offset = if pan_random > 0.0 { self.rand() * 5.0 * pan_random.clamp(0.0, 1.0) } else { 0.0 };
    }

    /// `classic` picks the MINI sound (the first-day engine) instead of the decoded original.
    pub fn render(&mut self, s: &VoiceSettings, sr: f32, classic: bool) -> f32 {
        if classic {
            let n = self.rand();
            return self.classic.render(s, sr, n);
        }
        if self.tone_env.idle() && self.noise_env.idle() {
            return 0.0;
        }
        let attack_step = 1.0 / (env_seconds(ATTACK_VOLTS) * sr);
        let tone_env = self.tone_env.next(attack_step, s.tone_decay, sr);
        let pitch_env = self.pitch_env.next(attack_step, s.pitch_decay, sr);
        let noise_env = self.noise_env.next(attack_step, s.noise_decay, sr);
        let filter_env = self.filter_env.next(attack_step, s.filter_decay, sr);

        // Phase-modulation oscillator (sine), output +-5 V.
        let pm_hz = volts_to_hz(s.pm_rate).min(sr * 0.45);
        self.pm_phase = (self.pm_phase + pm_hz / sr).fract();
        let pm_volts = (self.pm_phase * std::f32::consts::TAU).sin() * 5.0;
        // Phase Mod input -5..+5 V scaled by PM Depth 0..10 V (estimated: +-5 V = +-half cycle).
        let pm_cycles = (pm_volts / 10.0) * (s.pm_level / 10.0).clamp(0.0, 1.0);

        // Tone oscillator: pitch = Tone Tune + pitch envelope * Pitch Decay Vol (volts).
        let pitch_v = s.tone_tune + pitch_env * s.pitch_decay_vol;
        let hz = volts_to_hz(pitch_v).min(sr * 0.45);
        self.tone_phase = (self.tone_phase + hz / sr).fract();
        let tone = ((self.tone_phase + pm_cycles) * std::f32::consts::TAU).sin() * OSC_AMPLITUDE;
        let tone_out = tone * tone_env * (s.tone_volume / 10.0);

        // Noise oscillator (white) through the state-variable filter.
        let noise = self.rand() * OSC_AMPLITUDE;
        let cutoff_v = s.filter_freq + filter_env * FILTER_ENV_DEPTH_VOLTS;
        let fc = volts_to_hz(cutoff_v).min(sr * 0.5);
        let q = 2.0 - 2.0 * (s.filter_reso / 10.0).clamp(0.0, MAX_RESONANCE);
        let mut f = std::f32::consts::TAU * fc / sr;
        // Keep the recursion stable (the original limits f with a resonance table).
        let f_max = ((q * q + 4.0).sqrt() - q) * 0.95;
        if f > f_max {
            f = f_max;
        }
        let (low, high) = self.svf.process(noise, f, q);
        let filtered = if s.filter_hp >= FILTER_HP_THRESHOLD { high } else { low };
        // Noise VCA: exponential response (pin value 1); modelled as the square of the envelope.
        let noise_out = filtered * noise_env * noise_env * (s.noise_volume / 10.0);

        tone_out + noise_out
    }
}
