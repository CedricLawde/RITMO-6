//! Parameter identifiers and their ranges, shared by the plugin and the standalone app.
//! Knobs keep the original's units: volts 0..10 (pans -5..+5 V). See MECHANICS.md.

use crate::kits::{Kit, KITS};
use crate::presets::MINI_KIT;

pub const NUM_CH: usize = 6;
pub const NUM_STEPS: usize = 16;
pub const NUM_PROGS: usize = 16;
pub const NUM_KITS: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum VoiceParam {
    ToneTune,
    ToneDecay,
    ToneVolume,
    PitchDecay,
    PitchDecayVol,
    PhaseModRate,
    PhaseModLev,
    NoiseVolume,
    NoiseDecay,
    FilterFreq,
    FilterReso,
    FilterDecay,
    FilterHp,
    PanRandom,
}

/// Order matches `kits::Kit::voices` for the first 13 entries.
pub const VOICE_PARAMS: [VoiceParam; 14] = [
    VoiceParam::ToneTune,
    VoiceParam::ToneDecay,
    VoiceParam::ToneVolume,
    VoiceParam::PitchDecay,
    VoiceParam::PitchDecayVol,
    VoiceParam::PhaseModRate,
    VoiceParam::PhaseModLev,
    VoiceParam::NoiseVolume,
    VoiceParam::NoiseDecay,
    VoiceParam::FilterFreq,
    VoiceParam::FilterReso,
    VoiceParam::FilterDecay,
    VoiceParam::FilterHp,
    VoiceParam::PanRandom,
];

impl VoiceParam {
    pub fn index(self) -> usize {
        VOICE_PARAMS.iter().position(|p| *p == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ParamId {
    Level(usize),
    Pan(usize),
    Mute(usize),
    Master,
    Tempo,
    Sync,
    /// Sound engine: 0 = MINI (the first-day engine), 1 = ORIGINAL (decoded model).
    Sound,
    Voice(usize, VoiceParam),
}

pub const NUM_PARAMS: usize = 22 + NUM_CH * VOICE_PARAMS.len();

impl ParamId {
    pub fn index(self) -> usize {
        match self {
            ParamId::Level(c) => c,
            ParamId::Pan(c) => 6 + c,
            ParamId::Mute(c) => 12 + c,
            ParamId::Master => 18,
            ParamId::Tempo => 19,
            ParamId::Sync => 20,
            ParamId::Sound => 21,
            ParamId::Voice(c, p) => 22 + c * VOICE_PARAMS.len() + p.index(),
        }
    }

    pub fn all() -> Vec<ParamId> {
        let mut v = Vec::with_capacity(NUM_PARAMS);
        for c in 0..NUM_CH {
            v.push(ParamId::Level(c));
        }
        for c in 0..NUM_CH {
            v.push(ParamId::Pan(c));
        }
        for c in 0..NUM_CH {
            v.push(ParamId::Mute(c));
        }
        v.push(ParamId::Master);
        v.push(ParamId::Tempo);
        v.push(ParamId::Sync);
        v.push(ParamId::Sound);
        for c in 0..NUM_CH {
            for p in VOICE_PARAMS {
                v.push(ParamId::Voice(c, p));
            }
        }
        v
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Curve {
    Linear,
    Toggle,
}

/// How a value is shown to the person.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Show {
    Volts,
    /// 1 V/octave, 5 V = 440 Hz.
    PitchHz,
    /// Envelope stage length 2^(V - 20/3) s.
    EnvMs,
    Percent,
    LpHp,
    OnOff,
    Bpm,
    Sound,
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub name: &'static str,
    pub short: &'static str,
    pub min: f32,
    pub max: f32,
    pub default: f32,
    pub curve: Curve,
    pub show: Show,
}

impl Spec {
    pub fn to_norm(&self, plain: f32) -> f32 {
        let v = plain.clamp(self.min, self.max);
        (v - self.min) / (self.max - self.min)
    }

    pub fn from_norm(&self, n: f32) -> f32 {
        let n = n.clamp(0.0, 1.0);
        match self.curve {
            Curve::Linear => self.min + n * (self.max - self.min),
            Curve::Toggle => {
                if n >= 0.5 {
                    self.max
                } else {
                    self.min
                }
            }
        }
    }

    pub fn format(&self, plain: f32) -> String {
        match self.show {
            Show::Volts => format!("{plain:.2} V"),
            Show::PitchHz => {
                let hz = volts_to_hz(plain);
                if hz >= 1000.0 {
                    format!("{:.2} kHz ({plain:.2} V)", hz / 1000.0)
                } else {
                    format!("{hz:.1} Hz ({plain:.2} V)")
                }
            }
            Show::EnvMs => {
                let ms = env_seconds(plain) * 1000.0;
                if ms >= 1000.0 {
                    format!("{:.2} s ({plain:.2} V)", ms / 1000.0)
                } else {
                    format!("{ms:.1} ms ({plain:.2} V)")
                }
            }
            Show::Percent => format!("{:.0}%", plain * 100.0),
            Show::LpHp => (if plain >= FILTER_HP_THRESHOLD { "High-pass" } else { "Low-pass" }).to_string(),
            Show::OnOff => (if plain >= 0.5 { "On" } else { "Off" }).to_string(),
            Show::Bpm => format!("{:.0} BPM ({plain:.2} V)", internal_bpm(plain)),
            Show::Sound => (if plain >= 0.5 { "Original" } else { "Mini" }).to_string(),
        }
    }
}

pub const VOICE_LETTERS: [&str; NUM_CH] = ["A", "B", "C", "D", "E", "F"];

/// Pitch in SynthEdit volts to Hz: 1 V per octave, 5 V = 440 Hz (Ocet.dll 0x100af6a0 pin text,
/// cutoff table constant 440.0 at 0x1014f950).
pub fn volts_to_hz(v: f32) -> f32 {
    440.0 * (2.0f32).powf(v - 5.0)
}

/// Envelope stage length for a full 0..10 V sweep, in seconds:
/// 2^((V*1200 - 8000)/1200) (Ocet.dll 0x10081330).
pub fn env_seconds(v: f32) -> f32 {
    (2.0f32).powf(v - 20.0 / 3.0)
}

/// Clock module (OCET15 0x10001030): internal rate = V * 0.18 * 8 * 10 / 10 steps per second.
pub fn internal_steps_per_sec(v: f32) -> f32 {
    1.44 * v.max(0.5)
}

/// The same, as 16th-note BPM (4 steps per beat).
pub fn internal_bpm(v: f32) -> f32 {
    internal_steps_per_sec(v) * 60.0 / 4.0
}

/// Host-sync divider (OCET15 0x10001030): knob thresholds 3 V / 5 V / 6.9 V pick
/// 4, 6, 8 or 16 clocks per 2 beats, i.e. 2, 3, 4 or 8 steps per beat.
pub const SYNC_STEPS_PER_BEAT: [f64; 4] = [2.0, 3.0, 4.0, 8.0];
pub const SYNC_NAMES: [&str; 4] = ["1/8", "1/8T", "1/16", "1/32"];

pub fn sync_division(v: f32) -> usize {
    if v < 3.0 {
        0
    } else if v <= 5.0 {
        1
    } else if v <= 6.9 {
        2
    } else {
        3
    }
}

/// The LP/HP control is a two-state switch on the panel; values at or above this select high-pass.
pub const FILTER_HP_THRESHOLD: f32 = 5.0;

pub fn voice_spec(p: VoiceParam) -> Spec {
    let s = |name, short, show| Spec {
        name,
        short,
        min: 0.0,
        max: 10.0,
        default: 0.0,
        curve: Curve::Linear,
        show,
    };
    match p {
        VoiceParam::ToneTune => s("Tone Tune", "TUNE", Show::PitchHz),
        VoiceParam::ToneDecay => s("Tone Decay", "DECAY", Show::EnvMs),
        VoiceParam::ToneVolume => s("Tone Volume", "VOLUME", Show::Volts),
        VoiceParam::PitchDecay => s("Pitch Decay", "P.DECAY", Show::EnvMs),
        VoiceParam::PitchDecayVol => s("Pitch Decay Vol", "P.DEPTH", Show::Volts),
        VoiceParam::PhaseModRate => s("Phase Mod Rate", "PM RATE", Show::PitchHz),
        VoiceParam::PhaseModLev => s("Phase Mod Level", "PM LEV", Show::Volts),
        VoiceParam::NoiseVolume => s("Noise Volume", "VOLUME", Show::Volts),
        VoiceParam::NoiseDecay => s("Noise Decay", "DECAY", Show::EnvMs),
        VoiceParam::FilterFreq => s("Filter Frequency", "FREQ", Show::PitchHz),
        VoiceParam::FilterReso => s("Filter Resonance", "RESO", Show::Volts),
        VoiceParam::FilterDecay => s("Filter Decay", "F.DECAY", Show::EnvMs),
        VoiceParam::FilterHp => Spec {
            name: "Filter Lp/Hp",
            short: "LP/HP",
            min: 0.0,
            max: 10.0,
            default: 0.0,
            curve: Curve::Linear,
            show: Show::LpHp,
        },
        VoiceParam::PanRandom => Spec {
            name: "Pan Random",
            short: "PAN RND",
            min: 0.0,
            max: 1.0,
            default: 0.0,
            curve: Curve::Linear,
            show: Show::Percent,
        },
    }
}

/// Kits offered by the KIT selector: 0 = MINI (the first-day sounds), 1..=16 = factory kits.
pub const NUM_KIT_CHOICES: usize = NUM_KITS + 1;

pub fn kit_choice(i: usize) -> &'static Kit {
    if i == 0 {
        &MINI_KIT
    } else {
        &KITS[(i - 1).min(NUM_KITS - 1)]
    }
}

pub fn spec(id: ParamId) -> Spec {
    let k = &MINI_KIT;
    match id {
        ParamId::Level(c) => Spec {
            name: "Volume",
            short: "VOL",
            min: 0.0,
            max: 10.0,
            default: k.volume[c],
            curve: Curve::Linear,
            show: Show::Volts,
        },
        ParamId::Pan(c) => Spec {
            name: "Pan",
            short: "PAN",
            min: -5.0,
            max: 5.0,
            default: k.pan[c],
            curve: Curve::Linear,
            show: Show::Volts,
        },
        ParamId::Mute(_) => Spec {
            name: "Mute",
            short: "MUTE",
            min: 0.0,
            max: 1.0,
            default: 0.0,
            curve: Curve::Toggle,
            show: Show::OnOff,
        },
        ParamId::Master => Spec {
            name: "Master Volume",
            short: "MASTER",
            min: 0.0,
            max: 10.0,
            default: k.master,
            curve: Curve::Linear,
            show: Show::Volts,
        },
        ParamId::Tempo => Spec {
            name: "Tempo / Divider",
            short: "TEMPO",
            min: 0.5,
            max: 10.0,
            default: k.tempo,
            curve: Curve::Linear,
            show: Show::Bpm,
        },
        ParamId::Sync => Spec {
            name: "Host Sync",
            short: "SYNC",
            min: 0.0,
            max: 1.0,
            default: 0.0,
            curve: Curve::Toggle,
            show: Show::OnOff,
        },
        ParamId::Sound => Spec {
            name: "Sound",
            short: "SOUND",
            min: 0.0,
            max: 1.0,
            default: 0.0,
            curve: Curve::Toggle,
            show: Show::Sound,
        },
        ParamId::Voice(c, p) => {
            let mut s = voice_spec(p);
            s.default = if p == VoiceParam::PanRandom { 0.0 } else { k.voices[c][p.index()] };
            s
        }
    }
}

/// Read access to every parameter plus host-aware writes (begin / set / end gestures).
pub trait ParamAccess {
    fn get(&self, id: ParamId) -> f32;
    fn begin(&self, id: ParamId);
    fn set(&self, id: ParamId, plain: f32);
    fn end(&self, id: ParamId);

    fn set_gesture(&self, id: ParamId, plain: f32) {
        self.begin(id);
        self.set(id, plain);
        self.end(id);
    }
}

/// Load a kit choice (0 = MINI, 1..=16 = factory kits) into the parameters: sounds, volumes,
/// pans, master, tempo, and the matching sound engine.
pub fn apply_kit(access: &dyn ParamAccess, kit: usize) {
    let k = kit_choice(kit);
    access.set_gesture(ParamId::Sound, if kit == 0 { 0.0 } else { 1.0 });
    for c in 0..NUM_CH {
        for (i, p) in VOICE_PARAMS.iter().enumerate().take(13) {
            access.set_gesture(ParamId::Voice(c, *p), k.voices[c][i]);
        }
        access.set_gesture(ParamId::Level(c), k.volume[c]);
        access.set_gesture(ParamId::Pan(c), k.pan[c]);
    }
    access.set_gesture(ParamId::Master, k.master);
    access.set_gesture(ParamId::Tempo, k.tempo);
}
