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
    /// Column A: timing offset of this voice, -1..1 = up to half a step early / late.
    Nudge,
    /// Column B: this voice's share of SOUND DRIFT, 0..1 (default 100 %).
    DriftAmount,
}

/// Order matches `kits::Kit::voices` for the first 13 entries.
pub const VOICE_PARAMS: [VoiceParam; 16] = [
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
    VoiceParam::Nudge,
    VoiceParam::DriftAmount,
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
    /// Minipop-style timing wander (0..1); realigns every 128 steps.
    TempoDrift,
    /// Random variation of pitch, decay, level and filter on every hit (0..1).
    SoundDrift,
    /// Global swing: every second step is delayed by up to half a step (0..1).
    Swing,
    Voice(usize, VoiceParam),
    /// FX send of voice `c` to bus `k` (0 PHASER, 1 DIST, 2 DELAY, 3 SPRING), 0..1.
    Send(usize, usize),
    /// A setting of the shared FX section.
    Fx(FxParam),
}

/// The four shared FX buses, in panel order.
pub const NUM_FX_BUSES: usize = 4;
pub const FX_BUS_NAMES: [&str; NUM_FX_BUSES] = ["Phaser", "Distortion", "Delay", "Spring"];
pub const FX_BUS_SHORT: [&str; NUM_FX_BUSES] = ["PH", "DS", "DL", "SP"];

/// Every setting of the FX section (all 0..1 knobs unless noted).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum FxParam {
    PhRate,
    PhDepth,
    PhColor,
    PhMix,
    PhLevel,
    PhBypass,
    DsDrive,
    DsFilter,
    DsFat,
    DsMix,
    DsLevel,
    DsBypass,
    DlTime,
    DlFeedback,
    DlTone,
    DlAge,
    DlMod,
    DlMix,
    DlLevel,
    DlBypass,
    /// 0 TAPE, 1 BBD, 2 MOD.
    DlMode,
    /// on = SYNC (host / sequencer tempo), off = FREE (milliseconds).
    DlSync,
    SpDecay,
    SpDamping,
    SpTension,
    SpDrive,
    SpMix,
    SpLevel,
    SpBypass,
    /// Spring return high-pass (LOW CUT), 20 Hz (off) .. 1 kHz.
    SpLowCut,
    /// Spring return low-pass (HIGH CUT), 1 kHz .. 20 kHz (off).
    SpHighCut,
}

pub const FX_PARAMS: [FxParam; 31] = [
    FxParam::PhRate,
    FxParam::PhDepth,
    FxParam::PhColor,
    FxParam::PhMix,
    FxParam::PhLevel,
    FxParam::PhBypass,
    FxParam::DsDrive,
    FxParam::DsFilter,
    FxParam::DsFat,
    FxParam::DsMix,
    FxParam::DsLevel,
    FxParam::DsBypass,
    FxParam::DlTime,
    FxParam::DlFeedback,
    FxParam::DlTone,
    FxParam::DlAge,
    FxParam::DlMod,
    FxParam::DlMix,
    FxParam::DlLevel,
    FxParam::DlBypass,
    FxParam::DlMode,
    FxParam::DlSync,
    FxParam::SpDecay,
    FxParam::SpDamping,
    FxParam::SpTension,
    FxParam::SpDrive,
    FxParam::SpMix,
    FxParam::SpLevel,
    FxParam::SpBypass,
    FxParam::SpLowCut,
    FxParam::SpHighCut,
];
pub const NUM_FX_PARAMS: usize = FX_PARAMS.len();

impl FxParam {
    pub fn index(self) -> usize {
        FX_PARAMS.iter().position(|p| *p == self).unwrap_or(0)
    }
}

/// Delay SYNC divisions, in quarter notes (straight, dotted and triplet).
pub const DELAY_DIVISIONS: [(&str, f64); 13] = [
    ("1/32", 0.125),
    ("1/16T", 1.0 / 6.0),
    ("1/16", 0.25),
    ("1/8T", 1.0 / 3.0),
    ("1/16D", 0.375),
    ("1/8", 0.5),
    ("1/4T", 2.0 / 3.0),
    ("1/8D", 0.75),
    ("1/4", 1.0),
    ("1/2T", 4.0 / 3.0),
    ("1/4D", 1.5),
    ("1/2", 2.0),
    ("1/1", 4.0),
];

pub fn delay_division(v: f32) -> usize {
    ((v.clamp(0.0, 1.0) * (DELAY_DIVISIONS.len() - 1) as f32).round() as usize).min(DELAY_DIVISIONS.len() - 1)
}

/// FREE delay time: 20 ms .. 2 s.
pub fn delay_free_ms(v: f32) -> f32 {
    20.0 * (100.0f32).powf(v.clamp(0.0, 1.0))
}

/// Phaser rate: 0.03 .. 12 Hz.
pub fn phaser_hz(v: f32) -> f32 {
    0.03 * (400.0f32).powf(v.clamp(0.0, 1.0))
}

/// Spring LOW CUT (high-pass): 20 Hz (off) .. 1 kHz.
pub fn spring_low_cut_hz(v: f32) -> f32 {
    20.0 * (50.0f32).powf(v.clamp(0.0, 1.0))
}

/// Spring HIGH CUT (low-pass): 1 kHz .. 20 kHz (off).
pub fn spring_high_cut_hz(v: f32) -> f32 {
    1000.0 * (20.0f32).powf(v.clamp(0.0, 1.0))
}

/// FX level knob to gain: 0 .. x2 (+6 dB), 0.707 = unity.
pub fn fx_level_gain(v: f32) -> f32 {
    2.0 * v * v
}

pub const DELAY_MODE_NAMES: [&str; 3] = ["TAPE", "BBD", "MOD"];

pub const NUM_PARAMS: usize = 25 + NUM_CH * VOICE_PARAMS.len() + NUM_CH * NUM_FX_BUSES + NUM_FX_PARAMS;

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
            ParamId::TempoDrift => 22,
            ParamId::SoundDrift => 23,
            ParamId::Swing => 24,
            ParamId::Voice(c, p) => 25 + c * VOICE_PARAMS.len() + p.index(),
            ParamId::Send(c, k) => 25 + NUM_CH * VOICE_PARAMS.len() + c * NUM_FX_BUSES + k,
            ParamId::Fx(p) => 25 + NUM_CH * VOICE_PARAMS.len() + NUM_CH * NUM_FX_BUSES + p.index(),
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
        v.push(ParamId::TempoDrift);
        v.push(ParamId::SoundDrift);
        v.push(ParamId::Swing);
        for c in 0..NUM_CH {
            for p in VOICE_PARAMS {
                v.push(ParamId::Voice(c, p));
            }
        }
        for c in 0..NUM_CH {
            for k in 0..NUM_FX_BUSES {
                v.push(ParamId::Send(c, k));
            }
        }
        for p in FX_PARAMS {
            v.push(ParamId::Fx(p));
        }
        v
    }

    /// FX sends and FX settings: missing from files made before the FX expansion.
    pub fn is_fx(self) -> bool {
        matches!(self, ParamId::Send(..) | ParamId::Fx(_))
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
    /// Pitch with its note name (TUNE), e.g. "55.0 Hz · A1 +3 ct".
    PitchNote,
    /// Envelope stage length 2^(V - 20/3) s.
    EnvMs,
    Percent,
    LpHp,
    OnOff,
    Bpm,
    Sound,
    /// Two-sided drift: 50 % = none, below = late, above = early.
    Drift,
    /// -1..1 = half a step early .. half a step late.
    Nudge,
    /// 0..1 = straight .. 75 % swing.
    Swing,
    PhaserHz,
    LowCut,
    HighCut,
    DelayTime,
    DelayMode,
    FxLevel,
    Bypass,
    SyncFree,
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
            Show::PitchNote => {
                let (name, cents) = note_of_volts(plain);
                format!("{:.1} Hz · {name} {cents:+} ct ({plain:.2} V)", volts_to_hz(plain))
            }
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
            Show::Nudge => {
                let pct = (plain * 50.0).round();
                if pct.abs() < 1.0 {
                    "On the grid".to_string()
                } else if pct < 0.0 {
                    format!("Early {:.0}% of a step", -pct)
                } else {
                    format!("Late {pct:.0}% of a step")
                }
            }
            Show::Swing => {
                if plain < 0.005 {
                    "Straight (50%)".to_string()
                } else {
                    format!("{:.0}% swing", 50.0 + plain * 25.0)
                }
            }
            Show::PhaserHz => format!("{:.2} Hz", phaser_hz(plain)),
            Show::LowCut => if plain <= 0.001 { "OFF".to_string() } else { format!("{:.0} Hz", spring_low_cut_hz(plain)) },
            Show::HighCut => if plain >= 0.999 { "OFF".to_string() } else { format!("{:.1} kHz", spring_high_cut_hz(plain) / 1000.0) },
            Show::DelayTime => format!(
                "FREE {:.0} ms / SYNC {}",
                delay_free_ms(plain),
                DELAY_DIVISIONS[delay_division(plain)].0
            ),
            Show::DelayMode => DELAY_MODE_NAMES[(plain.round() as usize).min(2)].to_string(),
            Show::FxLevel => {
                let g = fx_level_gain(plain);
                if g <= 0.0001 {
                    "-inf dB".to_string()
                } else {
                    format!("{:+.1} dB", 20.0 * g.log10())
                }
            }
            Show::Bypass => (if plain >= 0.5 { "Bypassed" } else { "Active" }).to_string(),
            Show::SyncFree => (if plain >= 0.5 { "SYNC" } else { "FREE" }).to_string(),
            Show::LpHp => (if plain >= FILTER_HP_THRESHOLD { "High-pass" } else { "Low-pass" }).to_string(),
            Show::OnOff => (if plain >= 0.5 { "On" } else { "Off" }).to_string(),
            Show::Bpm => format!("{:.0} BPM ({plain:.2} V)", internal_bpm(plain)),
            Show::Sound => (if plain >= 0.5 { "Original" } else { "Mini" }).to_string(),
            Show::Drift => {
                let amount = ((plain - 0.5) * 200.0).round();
                if amount.abs() < 1.0 {
                    "No drift".to_string()
                } else if amount < 0.0 {
                    format!("Late (behind) {:.0}%", -amount)
                } else {
                    format!("Early (ahead) {amount:.0}%")
                }
            }
        }
    }
}

pub const VOICE_LETTERS: [&str; NUM_CH] = ["A", "B", "C", "D", "E", "F"];

/// Pitch in SynthEdit volts to Hz: 1 V per octave, 5 V = 440 Hz (Ocet.dll 0x100af6a0 pin text,
/// cutoff table constant 440.0 at 0x1014f950).
/// The nearest note to a pitch in volts (A4 = 440 Hz = 5 V, 1 V per octave) and how many
/// cents it is off, e.g. ("A1", 3).
pub fn note_of_volts(v: f32) -> (String, i32) {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let semis = (v - 5.0) * 12.0;
    let near = semis.round();
    let cents = ((semis - near) * 100.0).round() as i32;
    let midi = 69 + near as i32;
    (format!("{}{}", NAMES[midi.rem_euclid(12) as usize], midi.div_euclid(12) - 1), cents)
}

/// The pitch in volts of the nearest note (TUNE snapped to the semitone grid).
pub fn snap_to_note(v: f32) -> f32 {
    5.0 + ((v - 5.0) * 12.0).round() / 12.0
}

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
        VoiceParam::ToneTune => s("Tone Tune", "TUNE", Show::PitchNote),
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
        VoiceParam::Nudge => Spec {
            name: "Nudge",
            short: "NUDGE",
            min: -1.0,
            max: 1.0,
            default: 0.0,
            curve: Curve::Linear,
            show: Show::Nudge,
        },
        VoiceParam::DriftAmount => Spec {
            name: "Sound Drift Amount",
            short: "DRIFT",
            min: 0.0,
            max: 1.0,
            default: 1.0,
            curve: Curve::Linear,
            show: Show::Percent,
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
        // 50 % = no drift; towards 0 % steps fall randomly late (behind the beat),
        // towards 100 % they fall randomly early (ahead of the beat).
        ParamId::TempoDrift => Spec {
            name: "Tempo Drift",
            short: "T.DRIFT",
            min: 0.0,
            max: 1.0,
            default: 0.5,
            curve: Curve::Linear,
            show: Show::Drift,
        },
        ParamId::SoundDrift => Spec {
            name: "Sound Drift",
            short: "S.DRIFT",
            min: 0.0,
            max: 1.0,
            default: 0.0,
            curve: Curve::Linear,
            show: Show::Percent,
        },
        ParamId::Swing => Spec {
            name: "Swing",
            short: "SWING",
            min: 0.0,
            max: 1.0,
            default: 0.0,
            curve: Curve::Linear,
            show: Show::Swing,
        },
        ParamId::Send(_, k) => Spec {
            name: ["Phaser Send", "Distortion Send", "Delay Send", "Spring Send"][k.min(3)],
            short: FX_BUS_SHORT[k.min(3)],
            min: 0.0,
            max: 1.0,
            // sends start at zero: the sound is unchanged until you turn one up
            default: 0.0,
            curve: Curve::Linear,
            show: Show::Percent,
        },
        ParamId::Fx(p) => fx_spec(p),
        ParamId::Voice(c, p) => {
            let mut s = voice_spec(p);
            // kits hold the 13 sound knobs; Pan Random, Nudge and Drift keep their own defaults
            if p.index() < 13 {
                s.default = k.voices[c][p.index()];
            }
            s
        }
    }
}

pub fn fx_spec(p: FxParam) -> Spec {
    let knob = |name, short, default, show| Spec { name, short, min: 0.0, max: 1.0, default, curve: Curve::Linear, show };
    let toggle = |name, short, default, show| Spec { name, short, min: 0.0, max: 1.0, default, curve: Curve::Toggle, show };
    use FxParam::*;
    match p {
        PhRate => knob("Phaser Rate", "RATE", 0.4, Show::PhaserHz),
        PhDepth => knob("Phaser Depth", "DEPTH", 0.7, Show::Percent),
        PhColor => knob("Phaser Color", "COLOR", 0.4, Show::Percent),
        PhMix => knob("Phaser Mix", "MIX", 0.5, Show::Percent),
        PhLevel => knob("Phaser Level", "LEVEL", 0.707, Show::FxLevel),
        PhBypass => toggle("Phaser Bypass", "BYPASS", 0.0, Show::Bypass),
        DsDrive => knob("Distortion Drive", "DRIVE", 0.5, Show::Percent),
        DsFilter => knob("Distortion Filter", "FILTER", 0.4, Show::Percent),
        DsFat => knob("Distortion Fat", "FAT", 0.4, Show::Percent),
        DsMix => knob("Distortion Mix", "MIX", 1.0, Show::Percent),
        DsLevel => knob("Distortion Output", "OUTPUT", 0.6, Show::FxLevel),
        DsBypass => toggle("Distortion Bypass", "BYPASS", 0.0, Show::Bypass),
        DlTime => knob("Delay Time", "TIME", 0.58, Show::DelayTime),
        DlFeedback => knob("Delay Feedback", "FEEDBACK", 0.4, Show::Percent),
        DlTone => knob("Delay Tone", "TONE", 0.6, Show::Percent),
        DlAge => knob("Delay Age", "AGE", 0.3, Show::Percent),
        DlMod => knob("Delay Modulation", "MOD", 0.3, Show::Percent),
        DlMix => knob("Delay Mix", "MIX", 1.0, Show::Percent),
        DlLevel => knob("Delay Level", "LEVEL", 0.707, Show::FxLevel),
        DlBypass => toggle("Delay Bypass", "BYPASS", 0.0, Show::Bypass),
        DlMode => Spec { name: "Delay Mode", short: "MODE", min: 0.0, max: 2.0, default: 0.0, curve: Curve::Linear, show: Show::DelayMode },
        DlSync => toggle("Delay Sync", "SYNC", 1.0, Show::SyncFree),
        SpDecay => knob("Spring Decay", "DECAY", 0.5, Show::Percent),
        SpDamping => knob("Spring Damping", "DAMPING", 0.4, Show::Percent),
        SpTension => knob("Spring Tension", "TENSION", 0.5, Show::Percent),
        SpDrive => knob("Spring Drive", "DRIVE", 0.3, Show::Percent),
        SpMix => knob("Spring Mix", "MIX", 1.0, Show::Percent),
        SpLevel => knob("Spring Level", "LEVEL", 0.6, Show::FxLevel),
        SpBypass => toggle("Spring Bypass", "BYPASS", 0.0, Show::Bypass),
        SpLowCut => knob("Spring Low Cut", "LOW CUT", 0.0, Show::LowCut),
        SpHighCut => knob("Spring High Cut", "HIGH CUT", 1.0, Show::HighCut),
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

#[cfg(test)]
mod note_tests {
    use super::*;

    #[test]
    fn note_names_follow_a440() {
        assert_eq!(note_of_volts(5.0), ("A4".to_string(), 0));
        assert_eq!(note_of_volts(2.0), ("A1".to_string(), 0));
        assert_eq!(note_of_volts(5.0 - 9.0 / 12.0), ("C4".to_string(), 0));
        let (n, c) = note_of_volts(2.0 + 0.03 / 12.0 * 100.0 / 100.0);
        assert_eq!((n.as_str(), c), ("A1", 3));
        assert!((snap_to_note(2.02) - 2.0).abs() < 1e-6);
    }
}
