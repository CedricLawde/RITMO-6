//! MP7 sound: the third engine, in the spirit of the 1960s Mini Pops 7 rhythm box. Written from
//! scratch for Ritmo-6; no code or recordings from other software are in it. The way each voice
//! is built and its values follow a vintage rhythm machine of that family.
//!
//! Every voice plays one of 15 instruments, built from:
//! - an oscillator whose pitch starts SWEEP octaves up and falls back over SWEEP TIME, under a
//!   straight-line envelope (bass drum, congas, bongos; a narrow pulse for the rimshot),
//! - noise through a band filter with its own envelope (snares, hihat, cymbals, guiro, quijada),
//! - low-passed noise times a square wave (tambourine),
//! - a ringing resonator (claves, cowbell),
//! - a second resonator (FREQ 2), a tick (a very short click of noise),
//! - and, for the guiro and quijada, a train of clicks (RATE, DENSITY): the scrape and the rattle.
//!
//! The panel knobs keep their place; in MP7 mode they read (same slot, MP7 meaning):
//! TUNE = FREQ, DECAY = DECAY, VOLUME = LEVEL, P.DECAY = ATTACK, P.DEPTH = FREQ 2 / SWEEP,
//! PM RATE = RATE, PM LEV = DENSITY, NOISE VOLUME = LEVEL 2, NOISE DECAY = DECAY 2 / SWEEP TIME,
//! F.DECAY = TICK, FILTER FREQ = HIGHPASS, RESO = RESO (DRIVE on the tonal ones),
//! LP/HP = NORM / DARK.

use crate::dsp::VoiceSettings;
use crate::params::{env_seconds, volts_to_hz, FILTER_HP_THRESHOLD};

pub const NUM_INSTRUMENTS: usize = 15;

/// Instrument names, in selector order.
pub const INSTRUMENT_NAMES: [&str; NUM_INSTRUMENTS] = [
    "Bass Drum",
    "Snare 1",
    "Snare 2",
    "Rimshot",
    "Claves",
    "Cowbell",
    "Small Bongo",
    "Large Bongo",
    "Conga",
    "Hihat",
    "Cymbal 1",
    "Cymbal 2",
    "Tambourine",
    "Guiro",
    "Quijada",
];

/// Short names for the module's display.
pub const INSTRUMENT_SHORT: [&str; NUM_INSTRUMENTS] = [
    "BASS", "SNARE 1", "SNARE 2", "RIMSHOT", "CLAVES", "COWBELL", "S.BONGO", "L.BONGO", "CONGA", "HIHAT", "CYMBAL 1",
    "CYMBAL 2", "TAMBOUR.", "GUIRO", "QUIJADA",
];

pub const BASS: usize = 0;
pub const SNARE1: usize = 1;
pub const SNARE2: usize = 2;
pub const RIMSHOT: usize = 3;
pub const CLAVES: usize = 4;
pub const COWBELL: usize = 5;
pub const SMALL_BONGO: usize = 6;
pub const LARGE_BONGO: usize = 7;
pub const CONGA: usize = 8;
pub const HIHAT: usize = 9;
pub const CYMBAL1: usize = 10;
pub const CYMBAL2: usize = 11;
pub const TAMBOURINE: usize = 12;
pub const GUIRO: usize = 13;
pub const QUIJADA: usize = 14;

pub fn instrument_index(v: f32) -> usize {
    (v.round().max(0.0) as usize).min(NUM_INSTRUMENTS - 1)
}

#[derive(Clone, Copy, PartialEq)]
enum Wave {
    Sine,
    /// a narrow pulse (about 5 % duty): buzzy, for the rimshot's chirp
    NarrowPulse,
}

#[derive(Clone, Copy, PartialEq)]
enum Body {
    /// a ringing resonator at FREQ
    Reso,
    /// noise through a band filter at FREQ
    Noise,
    /// an oscillator at FREQ whose pitch starts SWEEP octaves higher and falls back over
    /// SWEEP TIME, under a straight-line envelope (bass drum, congas, bongos, rimshot)
    Tone(Wave),
    /// noise low-passed at FREQ 2 (a ratio of FREQ), multiplied by a square wave at FREQ: the
    /// tambourine's metallic jingle
    Ring,
}

#[derive(Clone, Copy, PartialEq)]
enum Train {
    Single,
    /// guiro: a steady scrape of clicks at RATE over LENGTH
    Scrape,
    /// quijada: a rattle of clicks at RATE dying away over LENGTH
    Rattle,
}

struct Kind {
    body: Body,
    train: Train,
    /// how far a strong hit bends the resonator's pitch up (fraction)
    bend: f32,
    /// transistor stage drive before the soft ceiling
    drive: f32,
    /// straight-line envelope (rise over ATTACK, then down to silence over DECAY), as the
    /// envelope generators of the reference rhythm box; otherwise an exponential decay
    linear: bool,
    /// the envelope drives an exponential-response amplifier (heard as its square)
    squared: bool,
}

fn kind(i: usize) -> Kind {
    let k = |body, train, bend, drive, linear, squared| Kind { body, train, bend, drive, linear, squared };
    match i {
        BASS | SMALL_BONGO | LARGE_BONGO | CONGA => k(Body::Tone(Wave::Sine), Train::Single, 0.0, 1.0, true, false),
        RIMSHOT => k(Body::Tone(Wave::NarrowPulse), Train::Single, 0.0, 1.0, true, false),
        SNARE1 | SNARE2 | CYMBAL1 | CYMBAL2 => k(Body::Noise, Train::Single, 0.0, 1.0, false, false),
        CLAVES | COWBELL => k(Body::Reso, Train::Single, 0.0, 1.0, false, false),
        HIHAT => k(Body::Noise, Train::Single, 0.0, 1.0, true, true),
        TAMBOURINE => k(Body::Ring, Train::Single, 0.0, 1.0, true, true),
        GUIRO => k(Body::Noise, Train::Scrape, 0.0, 1.2, false, false),
        _ => k(Body::Noise, Train::Rattle, 0.0, 2.0, false, false),
    }
}

/// Which knobs an instrument uses, as panel labels (None = not used by this instrument).
/// Order: the 13 sound knobs (tone tune .. lp/hp).
pub fn knob_labels(inst: usize) -> [Option<&'static str>; 13] {
    let k = kind(inst);
    let repeats = k.train != Train::Single;
    let decay_label = if repeats { "LENGTH" } else { "DECAY" };
    let (freq2, level2, decay2, reso) = match k.body {
        Body::Tone(_) => (Some("SWEEP"), None, Some("SWEEP TIME"), "DRIVE"),
        Body::Ring => (Some("NOISE BAND"), None, None, "RESO"),
        Body::Reso => (Some("FREQ 2"), Some("LEVEL 2"), Some("DECAY 2"), "DRIVE"),
        Body::Noise => (
            Some("FREQ 2"),
            Some("LEVEL 2"),
            Some(if k.train == Train::Rattle { "RING" } else { "DECAY 2" }),
            "RESO",
        ),
    };
    [
        Some("FREQ"),
        Some(decay_label),
        Some("LEVEL"),
        Some("ATTACK"),
        freq2,
        repeats.then_some("RATE"),
        repeats.then_some("DENSITY"),
        level2,
        decay2,
        Some("HIGHPASS"),
        Some(reso),
        Some("TICK"),
        Some("DARK"),
    ]
}

/// SWEEP of the oscillator instruments: knob volts to octaves (0..5).
pub fn sweep_octaves(v: f32) -> f32 {
    v.clamp(0.0, 10.0) * 0.5
}

// ------------------------------------------------------------------ knob conversions

/// ATTACK: trigger pulse length, 0.1 ms (0 V) .. 26 ms (10 V).
pub fn attack_seconds(v: f32) -> f32 {
    0.0001 * (2.0f32).powf(0.8 * v.clamp(0.0, 10.0))
}

/// FREQ 2 as a ratio of FREQ: 1/16 (0 V), 1 (5 V), 16 (10 V).
pub fn freq2_ratio(v: f32) -> f32 {
    (2.0f32).powf((v.clamp(0.0, 10.0) - 5.0) * 0.8)
}

fn hz_volts(hz: f32) -> f32 {
    (5.0 + (hz / 440.0).log2()).clamp(0.0, 10.0)
}
fn decay_volts(s: f32) -> f32 {
    (20.0 / 3.0 + s.log2()).clamp(0.0, 10.0)
}
fn attack_volts(s: f32) -> f32 {
    ((s / 0.0001).log2() / 0.8).clamp(0.0, 10.0)
}
fn ratio_volts(r: f32) -> f32 {
    (5.0 + r.log2() / 0.8).clamp(0.0, 10.0)
}

/// What a knob of an MP7 instrument is set to, in its own units (for the panel's hover text).
pub fn describe(inst: usize, slot: usize, v: f32) -> String {
    let hz = |h: f32| if h >= 1000.0 { format!("{:.2} kHz", h / 1000.0) } else { format!("{h:.1} Hz") };
    let ms = |s: f32| if s >= 1.0 { format!("{s:.2} s") } else { format!("{:.1} ms", s * 1000.0) };
    let tone = matches!(kind(inst.min(NUM_INSTRUMENTS - 1)).body, Body::Tone(_));
    match slot {
        0 | 5 | 9 => hz(volts_to_hz(v)),
        1 | 8 => ms(env_seconds(v)),
        3 => ms(attack_seconds(v)),
        4 if tone => format!("{:.2} octave(s) above FREQ at the start", sweep_octaves(v)),
        4 => format!("x{:.2} of FREQ", freq2_ratio(v)),
        12 => (if v >= FILTER_HP_THRESHOLD { "Dark" } else { "Normal" }).to_string(),
        _ => format!("{:.0}%", v * 10.0),
    }
}

/// An oscillator instrument's preset (bass drum, congas, bongos, rimshot), in the reference
/// box's own volts: pitch volts (1 V/octave, 5 V = 440 Hz), decay and sweep time volts of its
/// envelope generators (full sweep in 2^(V - 20/3) s), sweep depth in octaves.
#[allow(clippy::too_many_arguments)]
fn tone_knobs(pitch_v: f32, decay_v: f32, level: f32, attack: f32, sweep_oct: f32, sweep_v: f32, highpass: f32, drive: f32) -> [f32; 13] {
    [
        pitch_v,
        decay_v,
        level * 10.0,
        attack_volts(attack),
        (sweep_oct * 2.0).clamp(0.0, 10.0),
        0.0,
        0.0,
        0.0,
        sweep_v.clamp(0.0, 10.0),
        hz_volts(highpass),
        drive * 10.0,
        0.0,
        0.0,
    ]
}

/// An instrument's preset, as the 13 sound knob voltages.
#[allow(clippy::too_many_arguments)]
fn knobs(
    freq: f32,
    decay: f32,
    level: f32,
    attack: f32,
    ratio2: f32,
    rate: f32,
    density: f32,
    level2: f32,
    decay2: f32,
    highpass: f32,
    reso: f32,
    tick: f32,
    dark: bool,
) -> [f32; 13] {
    [
        hz_volts(freq),
        decay_volts(decay),
        level * 10.0,
        attack_volts(attack),
        ratio_volts(ratio2),
        if rate > 0.0 { hz_volts(rate) } else { 0.0 },
        density * 10.0,
        level2 * 10.0,
        decay_volts(decay2),
        hz_volts(highpass),
        reso * 10.0,
        tick * 10.0,
        if dark { 10.0 } else { 0.0 },
    ]
}

/// The values follow a vintage rhythm box of that family:
/// - oscillator voices: its voice settings (pitch, envelope and sweep volts of each voice);
/// - snares, cymbal 1, claves, cowbell, guiro, quijada: measured on its sounds (pitch,
///   spectrum, decay, rattle and scrape rates); cymbal 2 has no counterpart (estimate).
pub fn preset(inst: usize) -> [f32; 13] {
    match inst {
        // sine, 3 V minus TUNE 0.75 V = 2.25 V (65 Hz), sweep 0.6 octave with the 5 V envelope
        BASS => tone_knobs(2.25, 5.0, 1.0, 0.0025, 0.6, 5.0, 18.0, 0.15),
        // noise 2.7-7 kHz, -20 dB at 80 ms, -40 dB at 250 ms
        SNARE1 => knobs(4500.0, 0.38, 0.9, 0.0005, 1.0, 0.0, 0.0, 0.0, 0.02, 2200.0, 0.1, 0.15, false),
        // noise 3-7 kHz, peak at 6 ms, -20 dB at 105 ms, -40 dB at 290 ms
        SNARE2 => knobs(4800.0, 0.43, 0.9, 0.005, 1.0, 0.0, 0.0, 0.0, 0.02, 2500.0, 0.1, 0.1, false),
        // narrow pulse at 1.98 V (55 Hz) swept 5 octaves down in a few ms, 10 ms envelope
        RIMSHOT => tone_knobs(1.981, 0.0, 0.7, 0.0025, 5.0, 0.0, 100.0, 0.0),
        // 2153 Hz ring, -40 dB at 30 ms
        CLAVES => knobs(2153.0, 0.045, 0.8, 0.0001, 2.0, 0.0, 0.0, 0.0, 0.03, 120.0, 0.0, 0.05, false),
        // 851 Hz ring cut by a 25 ms envelope, with a 4.7 kHz click
        COWBELL => knobs(851.0, 0.06, 0.7, 0.0005, 5.54, 0.0, 0.0, 0.3, 0.012, 200.0, 0.0, 0.05, false),
        // the reference's high conga: 5.02 V, +0.42 octave over 5.43 V, 1.50 V envelope
        SMALL_BONGO => tone_knobs(5.018, 1.50, 0.85, 0.0049, 0.42, 5.43, 40.0, 0.1),
        // its low conga: 4.99 V, +0.31 octave blip, 1.39 V envelope
        LARGE_BONGO => tone_knobs(4.987, 1.39, 0.85, 0.0049, 0.313, 0.0, 40.0, 0.1),
        // its bongo, the lowest and longest drum: 3.74 V, +0.31 octave blip, 4.71 V envelope
        CONGA => tone_knobs(3.739, 4.71, 0.85, 0.0049, 0.313, 0.0, 30.0, 0.1),
        // noise band-passed at 8.53 V (5.1 kHz), resonance 0.54, 2.47 V envelope
        HIHAT => knobs(5130.0, 0.0546, 0.9, 0.0025, 1.0, 0.0, 0.0, 0.0, 0.02, 13.75, 0.54, 0.0, false),
        // like its long bright snare: noise 5-9 kHz, -40 dB at 0.68 s
        CYMBAL1 => knobs(7000.0, 1.0, 0.75, 0.005, 1.0, 0.0, 0.0, 0.0, 0.02, 4500.0, 0.15, 0.0, false),
        CYMBAL2 => knobs(9500.0, 0.45, 0.75, 0.0001, 1.0, 0.0, 0.0, 0.0, 0.02, 5000.0, 0.35, 0.0, false),
        // noise low-passed at 5.45 V (604 Hz, resonance 0.78) times a square at 8.53 V, 4.18 V envelope
        TAMBOURINE => knobs(5120.0, 0.178, 0.85, 0.0025, 0.118, 0.0, 0.0, 0.0, 0.02, 13.75, 0.78, 0.0, false),
        // noise clicks around 4.1 kHz at 20.4 a second, steady, for about 0.45 s
        GUIRO => knobs(4100.0, 0.45, 0.85, 0.0005, 1.0, 20.4, 0.72, 0.0, 0.02, 2000.0, 0.6, 0.1, false),
        // clicks around 2.6 kHz at 25 a second, each shorter than 10 ms, -30 dB after 0.6 s
        _ => knobs(2600.0, 1.5, 1.0, 0.0002, 1.08, 25.0, 0.2, 0.5, 0.0098, 1000.0, 0.75, 0.05, false),
    }
}

// ------------------------------------------------------------------ building blocks

/// Bridged-T style resonator: y = 2 r cos(w) y1 - r^2 y2 + x, with r giving -60 dB after the
/// ring time. An input of sin(w) (a unit impulse scaled) rings at amplitude 1.
#[derive(Clone, Copy, Default)]
struct Reso {
    y1: f32,
    y2: f32,
    /// rough amplitude follower, for the pitch bend of a strong hit
    amp: f32,
}

impl Reso {
    #[inline]
    fn tick(&mut self, x_area: f32, hz: f32, ring_s: f32, bend: f32, sr: f32) -> f32 {
        let f = (hz * (1.0 + bend * self.amp.min(1.5))).min(sr * 0.45).max(5.0);
        let w = std::f32::consts::TAU * f / sr;
        let r = (-6.907_755 / (ring_s.max(0.002) * sr)).exp();
        let y = 2.0 * r * w.cos() * self.y1 - r * r * self.y2 + x_area * w.sin();
        self.y2 = self.y1;
        self.y1 = y;
        self.amp = (self.amp * r).max(y.abs());
        y
    }

    fn quiet(&self) -> bool {
        self.y1.abs() < 1e-5 && self.y2.abs() < 1e-5
    }
}

/// TPT state-variable filter (band-pass with unity peak, and the high-pass of the click).
#[derive(Clone, Copy, Default)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    #[inline]
    fn band(&mut self, x: f32, hz: f32, reso: f32, sr: f32) -> f32 {
        let g = (std::f32::consts::PI * hz.clamp(20.0, sr * 0.45) / sr).tan();
        let k = 2.0 - 1.9 * reso.clamp(0.0, 1.0);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v1 * k
    }

    /// Low-pass output (resonance up to 0.95).
    #[inline]
    fn low(&mut self, x: f32, hz: f32, reso: f32, sr: f32) -> f32 {
        let g = (std::f32::consts::PI * hz.clamp(20.0, sr * 0.45) / sr).tan();
        let k = 2.0 - 1.9 * reso.clamp(0.0, 1.0);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v2
    }
}

/// The reference box's envelope: a straight rise over `attack_s`, then a straight fall to
/// silence over `decay_s`, `t` seconds after the hit.
#[inline]
fn line_env(t: f32, attack_s: f32, decay_s: f32) -> f32 {
    if t < attack_s {
        t / attack_s.max(1e-6)
    } else {
        (1.0 - (t - attack_s) / decay_s.max(1e-4)).max(0.0)
    }
}

/// One-pole filters (output high-pass, DARK low-pass, click high-pass).
#[derive(Clone, Copy, Default)]
struct OnePole {
    z: f32,
}

impl OnePole {
    #[inline]
    fn low(&mut self, x: f32, hz: f32, sr: f32) -> f32 {
        let a = 1.0 - (-std::f32::consts::TAU * hz / sr).exp();
        self.z += a * (x - self.z);
        self.z
    }
    #[inline]
    fn high(&mut self, x: f32, hz: f32, sr: f32) -> f32 {
        x - self.low(x, hz, sr)
    }
}

/// Envelope of the noise part: a linear rise over ATTACK to the pulse's level, then an
/// exponential fall (-60 dB after the decay time).
#[derive(Clone, Copy, Default)]
struct Env {
    level: f32,
    target: f32,
    rise: f32,
    rise_left: u32,
}

impl Env {
    fn hit(&mut self, amp: f32, attack_samples: f32) {
        let n = attack_samples.max(1.0) as u32;
        let target = amp.max(self.level);
        self.target = target;
        self.rise_left = n;
        self.rise = (target - self.level) / n as f32;
    }

    #[inline]
    fn next(&mut self, decay_s: f32, sr: f32) -> f32 {
        if self.rise_left > 0 {
            self.level += self.rise;
            self.rise_left -= 1;
            if self.rise_left == 0 {
                self.level = self.target;
            }
        } else {
            self.level *= (-6.907_755 / (decay_s.max(0.001) * sr)).exp();
        }
        self.level
    }
}

/// The trigger pulse: a half-sine of ATTACK length with unit area (so a short pulse hits like an
/// impulse, a long one rounds the sound off, as a slower trigger would).
#[derive(Clone, Copy, Default)]
struct Pulse {
    pos: u32,
    len: u32,
    amp: f32,
}

impl Pulse {
    fn start(&mut self, amp: f32, len_samples: f32) {
        // a pulse still running adds to the new one (a fast rattle)
        self.len = len_samples.max(1.0) as u32;
        self.pos = 0;
        self.amp = amp;
    }

    #[inline]
    fn next(&mut self) -> f32 {
        if self.pos >= self.len {
            return 0.0;
        }
        let l = self.len as f32;
        let x = (std::f32::consts::PI * (self.pos as f32 + 0.5) / l).sin() * std::f32::consts::FRAC_PI_2 / l;
        self.pos += 1;
        x * self.amp
    }
}

// ------------------------------------------------------------------ the voice

#[derive(Clone, Copy, Default)]
pub struct Mp7 {
    active: bool,
    /// samples since the hit
    t: u32,
    /// pulses struck so far, and the sample of the next one
    struck: u32,
    next_pulse: f32,
    /// the quijada's second click inside one period (sample, level), 0 = none
    sub_at: f32,
    sub_amp: f32,
    pulse: Pulse,
    res1: Reso,
    res2: Reso,
    band: Svf,
    env: Env,
    tick_env: f32,
    tick_hp: OnePole,
    out_hp: OnePole,
    out_hp2: OnePole,
    dark: OnePole,
    /// gentle top cut of the noise voices (two poles, half an octave above their band)
    noise_lp: OnePole,
    noise_lp2: OnePole,
    /// oscillator phases (the drums' tone, the tambourine's square)
    phase: f32,
    /// the last straight-line envelope level (to know when it has finished)
    line: f32,
    /// small part-to-part differences of every hit, as on an old box (pitch and level)
    detune: f32,
    gain: f32,
    rng: u32,
    quiet_for: u32,
}

impl Mp7 {
    pub fn trigger(&mut self, seed: u32) {
        if self.rng == 0 {
            self.rng = seed | 1;
        }
        self.active = true;
        self.t = 0;
        self.struck = 0;
        self.next_pulse = 0.0;
        self.sub_at = 0.0;
        self.quiet_for = 0;
        self.phase = 0.0;
        self.line = 1.0;
        self.detune = 1.0 + 0.002 * self.rand();
        self.gain = 1.0 + 0.03 * self.rand();
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

    /// Strike the parts of the instrument once (one pulse of the train).
    fn strike(&mut self, amp: f32, s: &VoiceSettings, k: &Kind, sr: f32) {
        let att = attack_seconds(s.pitch_decay) * sr;
        self.pulse.start(amp, att);
        if k.body == Body::Noise {
            self.env.hit(amp, att);
        }
        self.tick_env = self.tick_env.max(amp);
    }

    /// Run the pulse train: strike every pulse that is due at sample `t`.
    fn train(&mut self, inst: usize, s: &VoiceSettings, k: &Kind, sr: f32) {
        let t = self.t as f32;
        if self.sub_at > 0.0 && t >= self.sub_at {
            let a = self.sub_amp;
            self.sub_at = 0.0;
            self.strike(a, s, k, sr);
        }
        if t < self.next_pulse {
            return;
        }
        let density = (s.pm_level / 10.0).clamp(0.0, 1.0);
        let period = sr / volts_to_hz(s.pm_rate).clamp(2.0, 400.0);
        let n = self.struck;
        let length = env_seconds(s.tone_decay) * sr;
        let amp = match k.train {
            Train::Single => {
                if n > 0 {
                    self.next_pulse = f32::MAX;
                    return;
                }
                1.0
            }
            Train::Scrape => {
                if t > length {
                    self.next_pulse = f32::MAX;
                    return;
                }
                // a steady scrape: every tooth about as loud, a quick fade in and out
                let fade_in = (t / (0.015 * sr)).min(1.0);
                let fade_out = ((length - t) / (0.03 * sr)).clamp(0.0, 1.0);
                (fade_in.max(0.5) * fade_out.max(0.2)) * (0.9 + 0.1 * self.rand())
            }
            Train::Rattle => {
                // a strong first hit, then the teeth rattle away
                let a = (-6.907_755 * t / length.max(1.0)).exp();
                if a < 0.002 {
                    self.next_pulse = f32::MAX;
                    return;
                }
                if n == 0 { 1.0 } else { a * (0.75 + 0.25 * self.rand().abs()) }
            }
        };
        self.strike(amp, s, k, sr);
        self.struck += 1;
        let _ = inst;
        // when the next pulse comes: steadier at low density, looser and doubled at high density
        let jitter = match k.train {
            Train::Rattle => 0.04 + 0.18 * density,
            Train::Scrape => 0.03 + 0.1 * (1.0 - density),
            _ => 0.15,
        };
        let gap = period * (1.0 + jitter * self.rand());
        self.next_pulse = t + gap.max(2.0);
        if k.train == Train::Rattle && density > 0.05 && n > 0 {
            self.sub_at = t + gap * (0.42 + 0.1 * self.rand());
            self.sub_amp = amp * density * 0.6;
        }
    }

    pub fn render(&mut self, s: &VoiceSettings, inst: usize, sr: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let inst = inst.min(NUM_INSTRUMENTS - 1);
        let k = kind(inst);
        self.train(inst, s, &k, sr);
        self.t = self.t.saturating_add(1);

        let freq = volts_to_hz(s.tone_tune) * self.detune;
        let level = (s.tone_volume / 10.0).clamp(0.0, 1.0);
        let decay = env_seconds(s.tone_decay);
        let freq2 = freq * freq2_ratio(s.pitch_decay_vol);
        let level2 = (s.noise_volume / 10.0).clamp(0.0, 1.0);
        let decay2 = env_seconds(s.noise_decay);
        let reso = (s.filter_reso / 10.0).clamp(0.0, 1.0);
        let tick = (s.filter_decay / 10.0).clamp(0.0, 1.0);
        let x = self.pulse.next();

        // the straight-line envelope of the oscillator, hi-hat and tambourine voices
        let secs = self.t as f32 / sr;
        let attack = attack_seconds(s.pitch_decay);
        let mut line = 1.0;
        if k.linear {
            line = line_env(secs, attack, decay);
            self.line = line;
            if k.squared {
                line *= line;
            }
        }

        // main part
        let main = match k.body {
            Body::Reso => {
                // a rattle's teeth ring short (RING); a single hit rings over DECAY
                let ring = if k.train == Train::Rattle { decay2 } else { decay };
                self.res1.tick(x, freq, ring, k.bend, sr)
            }
            Body::Noise => {
                let n = self.rand();
                let e = if k.linear {
                    line
                } else {
                    let per_click = match k.train {
                        Train::Scrape => {
                            let period = 1.0 / volts_to_hz(s.pm_rate).clamp(2.0, 400.0);
                            period * (0.25 + 0.9 * (s.pm_level / 10.0).clamp(0.0, 1.0))
                        }
                        Train::Rattle => decay2,
                        Train::Single => decay,
                    };
                    self.env.next(per_click, sr)
                };
                let b = self.band.band(n, freq, reso, sr);
                let top = (freq * 1.5).min(sr * 0.45);
                let b = self.noise_lp.low(b, top, sr);
                self.noise_lp2.low(b, top, sr) * e * 3.0
            }
            Body::Tone(wave) => {
                // pitch starts SWEEP octaves up and comes down in a straight line (in volts)
                // over SWEEP TIME, after the same attack
                let sweep = line_env(secs, attack, decay2);
                let f = (freq * (2.0f32).powf(sweep_octaves(s.pitch_decay_vol) * sweep)).min(sr * 0.45);
                self.phase = (self.phase + f / sr).fract();
                let osc = match wave {
                    Wave::Sine => (self.phase * std::f32::consts::TAU).sin(),
                    Wave::NarrowPulse => {
                        if self.phase < 0.05 {
                            0.95
                        } else {
                            -0.05
                        }
                    }
                };
                osc * line
            }
            Body::Ring => {
                // noise low-passed at FREQ 2, times a square at FREQ
                let n = self.rand();
                let band = self.band.low(n, freq2, reso.min(0.95), sr);
                self.phase = (self.phase + freq.min(sr * 0.45) / sr).fract();
                let square = if self.phase < 0.5 { 1.0 } else { -1.0 };
                band * square * line * 4.0
            }
        };
        // second resonator (body / second tone / overtone), excited by the same pulse
        let ring2 = if k.train == Train::Rattle { decay2 * 0.8 } else { decay2 };
        let has_second = matches!(k.body, Body::Reso | Body::Noise);
        let second = if has_second && level2 > 0.0005 { self.res2.tick(x, freq2, ring2, k.bend * 0.5, sr) } else { 0.0 };
        // tick: about 1.5 ms of high-passed noise
        let mut click = 0.0;
        if self.tick_env > 1e-4 && tick > 0.0 {
            let n = self.rand();
            click = self.tick_hp.high(n, 2500.0, sr) * self.tick_env * tick * 1.2;
        }
        self.tick_env *= (-1.0 / (0.0015 * sr)).exp();

        let mut y = main * level + second * level2 + click;
        // transistor stage: drive (RESO = DRIVE on the resonant instruments) into a soft ceiling
        let drive = k.drive * if matches!(k.body, Body::Reso | Body::Tone(_)) { 1.0 + 2.5 * reso } else { 1.0 };
        y = (drive * y).tanh() / drive.tanh().max(0.3) * 0.85 / (1.0 + 0.15 * (drive - 1.0).max(0.0));
        // HIGHPASS (two gentle poles) and DARK
        let hp = volts_to_hz(s.filter_freq).clamp(5.0, sr * 0.45);
        y = self.out_hp.high(y, hp, sr);
        y = self.out_hp2.high(y, hp * 0.7, sr);
        if s.filter_hp >= FILTER_HP_THRESHOLD {
            y = self.dark.low(y, 3500.0, sr);
        }
        y *= self.gain;

        // fall asleep once everything has died away and no pulse is left to come
        let pending = self.next_pulse < f32::MAX || self.sub_at > 0.0 || self.pulse.pos < self.pulse.len;
        let line_done = !k.linear || (self.line <= 0.0 && secs > attack);
        if !pending && line_done && y.abs() < 1e-5 && self.res1.quiet() && self.res2.quiet() && self.env.level < 1e-5 {
            self.quiet_for += 1;
            if self.quiet_for > 64 {
                self.active = false;
            }
        } else {
            self.quiet_for = 0;
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(inst: usize) -> VoiceSettings {
        let mut a = [0.0f32; 16];
        a[..13].copy_from_slice(&preset(inst));
        VoiceSettings::from_array(&a)
    }

    /// Peak and the times (seconds) at which the sound is last above -40 dB of its peak.
    fn run(inst: usize, seconds: f32) -> (f32, f32, Vec<f32>) {
        let sr = 48000.0;
        let s = settings(inst);
        let mut v = Mp7::default();
        v.trigger(0x1234_5678);
        let out: Vec<f32> = (0..(seconds * sr) as usize).map(|_| v.render(&s, inst, sr)).collect();
        let peak = out.iter().fold(0.0f32, |m, y| m.max(y.abs()));
        let last = out.iter().rposition(|y| y.abs() > peak * 0.01).unwrap_or(0) as f32 / sr;
        (peak, last, out)
    }

    #[test]
    fn every_instrument_sounds_and_stays_in_range() {
        for i in 0..NUM_INSTRUMENTS {
            let (peak, last, out) = run(i, 2.0);
            assert!(out.iter().all(|y| y.is_finite()), "{}", INSTRUMENT_NAMES[i]);
            assert!(peak > 0.05, "{} too quiet: {peak}", INSTRUMENT_NAMES[i]);
            assert!(peak < 1.6, "{} too loud: {peak}", INSTRUMENT_NAMES[i]);
            assert!(last > 0.005 && last < 1.9, "{} length {last}", INSTRUMENT_NAMES[i]);
        }
    }

    #[test]
    fn bass_drum_rings_at_its_frequency() {
        let (_, _, out) = run(BASS, 0.4);
        // count zero crossings between 50 and 250 ms
        let a = (0.05 * 48000.0) as usize;
        let b = (0.25 * 48000.0) as usize;
        let crossings = out[a..b].windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let hz = crossings as f32 / 0.2;
        assert!((60.0..95.0).contains(&hz), "bass at {hz} Hz");
    }

    #[test]
    fn quijada_rattles() {
        // the rattle is a train of separate clicks: count the bursts in the envelope
        let (peak, last, out) = run(QUIJADA, 1.5);
        assert!(last > 0.3, "the rattle should last: {last}");
        let win = 240; // 5 ms
        let env: Vec<f32> = out.chunks(win).map(|c| c.iter().fold(0.0f32, |m, y| m.max(y.abs()))).collect();
        let mut bursts = 0;
        let mut high = false;
        for e in &env {
            if !high && *e > peak * 0.08 {
                bursts += 1;
                high = true;
            } else if high && *e < peak * 0.04 {
                high = false;
            }
        }
        assert!(bursts >= 8, "quijada bursts: {bursts}");
        // and it dies away: the last quarter is far quieter than the first
        let q = env.len() / 4;
        let first = env[..q].iter().cloned().fold(0.0, f32::max);
        let lastq = env[3 * q..].iter().cloned().fold(0.0, f32::max);
        assert!(lastq < first * 0.2, "{lastq} vs {first}");
    }

    #[test]
    fn guiro_scrapes_for_its_length() {
        let (_, last, _) = run(GUIRO, 1.0);
        assert!((0.3..0.6).contains(&last), "guiro length {last}");
    }

    /// Listening aid: `RITMO6_WAV_DIR=<folder> cargo test --release mp7_wavs -- --ignored`
    /// writes one WAV per instrument (48 kHz, 16-bit mono) into that folder.
    #[test]
    #[ignore]
    fn mp7_wavs() {
        let Ok(dir) = std::env::var("RITMO6_WAV_DIR") else { return };
        for i in 0..NUM_INSTRUMENTS {
            let (peak, last, out) = run(i, 1.5);
            let mut b: Vec<u8> = Vec::new();
            let n = out.len() as u32;
            let w = |b: &mut Vec<u8>, x: &[u8]| b.extend_from_slice(x);
            w(&mut b, b"RIFF");
            w(&mut b, &(36 + n * 2).to_le_bytes());
            w(&mut b, b"WAVEfmt ");
            w(&mut b, &16u32.to_le_bytes());
            w(&mut b, &1u16.to_le_bytes());
            w(&mut b, &1u16.to_le_bytes());
            w(&mut b, &48000u32.to_le_bytes());
            w(&mut b, &96000u32.to_le_bytes());
            w(&mut b, &2u16.to_le_bytes());
            w(&mut b, &16u16.to_le_bytes());
            w(&mut b, b"data");
            w(&mut b, &(n * 2).to_le_bytes());
            for y in &out {
                w(&mut b, &((y.clamp(-1.0, 1.0) * 32000.0) as i16).to_le_bytes());
            }
            let name = format!("{dir}/{:02}-{}.wav", i + 1, INSTRUMENT_NAMES[i].replace(' ', "-"));
            std::fs::write(&name, b).unwrap();
            let zc = out.windows(2).take((0.1 * 48000.0) as usize).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
            eprintln!("{:12} peak {peak:.2} length {last:.3} s, ~{} Hz in the first 100 ms", INSTRUMENT_NAMES[i], zc * 10);
        }
    }

    #[test]
    fn presets_fit_the_knobs() {
        for i in 0..NUM_INSTRUMENTS {
            for v in preset(i) {
                assert!((0.0..=10.0).contains(&v), "{} {v}", INSTRUMENT_NAMES[i]);
            }
        }
    }
}
