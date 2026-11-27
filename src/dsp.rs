//! One drum voice, built like the original's sound module:
//! a sine tone oscillator (pitch envelope + phase modulation from a second sine),
//! and white noise through a state-variable filter, each with its own envelope.
//! Conversions follow the original engine (see MECHANICS.md for addresses).

use crate::classic::Classic;
use crate::mp7::Mp7;
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
    /// timing offset, -1..1 = half a step early / late (column A)
    pub nudge: f32,
    /// share of SOUND DRIFT for this voice, 0..1 (column B)
    pub drift_amount: f32,
    /// MP7 engine: the instrument this voice plays (0..14, see mp7::INSTRUMENT_NAMES)
    pub mp7_inst: f32,
}

/// The three sound engines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SoundEngine {
    /// MINI: the punchy first-day engine (src/classic.rs)
    #[default]
    Mini,
    /// ORIGINAL: the model decoded from the original plugin
    Original,
    /// MP7: Mini Pops 7 style instruments (src/mp7.rs)
    Mp7,
}

impl VoiceSettings {
    pub fn from_array(v: &[f32; 16]) -> Self {
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
            nudge: v[14],
            drift_amount: v[15],
            mp7_inst: 0.0,
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
    mp7: Mp7,
    /// Retrigger overload, as on the TR-808 / TR-909 (see SATURATION.md): every hit adds one
    /// unit of charge on top of what is left from the previous hits, and the charge leaks away
    /// with a time constant tied to the voice's own decay. An isolated hit leaves 1 (clean);
    /// hits that come back before the voice has died down pile up, and only the excess over 1
    /// drives a soft saturation, so it grows gradually with how fast and how long the hits are.
    charge: f32,
    /// SOUND DRIFT offsets drawn at the last hit.
    drift_pitch_v: f32,
    drift_decay_v: f32,
    drift_cutoff_v: f32,
    drift_gain: f32,
    /// ACCENT: level of the current hit (1 = plain step).
    accent_gain: f32,
}

/// The output stage's soft ceiling: untouched up to 0.9, then rounding off towards 1.6.
#[inline]
fn overload_clip(u: f32) -> f32 {
    const KNEE: f32 = 0.9;
    const ROOM: f32 = 0.7;
    let m = u.abs();
    if m <= KNEE {
        u
    } else {
        (KNEE + ROOM * ((m - KNEE) / ROOM).tanh()).copysign(u)
    }
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
            mp7: Mp7::default(),
            charge: 0.0,
            drift_pitch_v: 0.0,
            drift_decay_v: 0.0,
            drift_cutoff_v: 0.0,
            drift_gain: 1.0,
            accent_gain: 1.0,
        }
    }

    /// Strike the voice.
    /// - `pan_random` 0..1 widens a random pan offset (up to +-5 V).
    /// - `sound_drift` 0..1: random pitch (up to +-1.4 semitones), decay (+-0.5 V, about x/1.4),
    ///   cutoff (+-0.6 V) and level (+-25 %) for this hit.
    /// - `stacked`: an extra MULTIPLY hit inside a step. Every hit adds charge the same way
    ///   (quick normal steps too); only the timing decides how much is left.
    /// - `accent` 0..1: 0 for a plain hit; for an accented step, the ACCENT knob. The hit plays
    ///   up to 1.6x louder (+4 dB) and adds up to half a unit of extra charge, so an accent
    ///   warms the sound a little on its own and pushes a fast roll further into saturation.
    pub fn strike(&mut self, pan_random: f32, sound_drift: f32, stacked: bool, accent: f32) {
        self.trigger(pan_random);
        let d = sound_drift.clamp(0.0, 1.0);
        if d > 0.0 {
            self.drift_pitch_v = self.rand() * 0.12 * d;
            self.drift_decay_v = self.rand() * 0.5 * d;
            self.drift_cutoff_v = self.rand() * 0.6 * d;
            self.drift_gain = 1.0 + self.rand() * 0.25 * d;
        } else {
            self.drift_pitch_v = 0.0;
            self.drift_decay_v = 0.0;
            self.drift_cutoff_v = 0.0;
            self.drift_gain = 1.0;
        }
        let _ = stacked;
        let a = accent.clamp(0.0, 1.0);
        self.accent_gain = crate::params::accent_gain(a);
        self.charge = (self.charge + 1.0 + 0.5 * a).min(8.0);
    }

    /// Render one sample with SOUND DRIFT and MULTIPLY saturation applied around the engine.
    pub fn render(&mut self, s: &VoiceSettings, sr: f32, engine: SoundEngine) -> f32 {
        let mut d = *s;
        if self.drift_gain != 1.0 || self.drift_pitch_v != 0.0 {
            d.tone_tune += self.drift_pitch_v;
            d.pm_rate += self.drift_pitch_v;
            d.tone_decay += self.drift_decay_v;
            d.noise_decay += self.drift_decay_v;
            d.filter_freq += self.drift_cutoff_v;
        }
        let raw = self.render_engine(&d, sr, engine) * self.drift_gain * self.accent_gain;

        // leak: a fifth of the voice's decay time (15..300 ms), so a short hat recovers in a
        // few milliseconds and a long kick keeps some charge for a whole 16th note
        if self.charge > 1e-4 {
            let tau = (env_seconds(s.tone_decay.max(s.noise_decay)) * 0.2).clamp(0.015, 0.3);
            self.charge *= (-1.0 / (tau * sr)).exp();
        } else {
            self.charge = 0.0;
        }
        // only charge above one hit's worth overloads; nothing happens below it
        let heat = (self.charge - 1.0).max(0.0);
        if heat <= 1e-4 {
            return raw;
        }
        // the piled-up energy pushes the voice into the output stage's soft ceiling (about 1.0):
        // more drive (level and odd harmonics) as the heat rises, faded in smoothly from zero so
        // a little overlap only warms the sound and a fast roll really crunches
        let w = 1.0 - (-2.0 * heat).exp();
        let g = 1.0 + 0.6 * heat.min(4.0);
        raw + w * (overload_clip(g * raw) - raw)
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

    fn trigger(&mut self, pan_random: f32) {
        self.tone_env.trigger();
        self.pitch_env.trigger();
        self.noise_env.trigger();
        self.filter_env.trigger();
        self.classic.trigger();
        let seed = self.rng;
        self.mp7.trigger(seed);
        self.pan_offset = if pan_random > 0.0 { self.rand() * 5.0 * pan_random.clamp(0.0, 1.0) } else { 0.0 };
    }

    /// MINI (the first-day engine), the decoded ORIGINAL, or MP7 (Mini Pops 7 style).
    fn render_engine(&mut self, s: &VoiceSettings, sr: f32, engine: SoundEngine) -> f32 {
        match engine {
            SoundEngine::Mini => {
                let n = self.rand();
                return self.classic.render(s, sr, n);
            }
            SoundEngine::Mp7 => {
                return self.mp7.render(s, crate::mp7::instrument_index(s.mp7_inst), sr);
            }
            SoundEngine::Original => {}
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
