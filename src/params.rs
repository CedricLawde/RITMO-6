//! Parameter identifiers and their ranges, shared by the plugin and the standalone app.
//! Knobs keep the original's units: volts 0..10 (pans -5..+5 V). See MECHANICS.md.

use crate::kits::{Kit, KITS};
use crate::presets::{MINI_KIT, MP7_INSTRUMENTS, MP7_KIT};

pub const NUM_CH: usize = 6;
pub const NUM_STEPS: usize = 16;
pub const NUM_PROGS: usize = 16;
pub const NUM_KITS: usize = 16;
/// The KIT selector's MP7 kit (after MINI and the 16 factory kits).
pub const MP7_KIT_CHOICE: usize = NUM_KITS + 1;

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
    /// ACCENT: how much louder (and hotter) an accented step plays (0..1). Added in v2, so it
    /// comes last in the parameter list and older projects keep their numbering.
    Accent,
    Voice(usize, VoiceParam),
    /// MP7 sound engine on (added in 2.4, after ACCENT): it takes the place of MINI / ORIGINAL.
    Mp7,
    /// The MP7 instrument voice `c` plays (0..14, see mp7::INSTRUMENT_NAMES). Added in 2.4.
    Mp7Inst(usize),
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

pub const NUM_PARAMS: usize = 27 + NUM_CH + NUM_CH * VOICE_PARAMS.len() + NUM_CH * NUM_FX_BUSES + NUM_FX_PARAMS;

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
            ParamId::Accent => 25 + NUM_CH * VOICE_PARAMS.len() + NUM_CH * NUM_FX_BUSES + NUM_FX_PARAMS,
            ParamId::Mp7 => 26 + NUM_CH * VOICE_PARAMS.len() + NUM_CH * NUM_FX_BUSES + NUM_FX_PARAMS,
            ParamId::Mp7Inst(c) => 27 + NUM_CH * VOICE_PARAMS.len() + NUM_CH * NUM_FX_BUSES + NUM_FX_PARAMS + c,
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
        v.push(ParamId::Accent);
        v.push(ParamId::Mp7);
        for c in 0..NUM_CH {
            v.push(ParamId::Mp7Inst(c));
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
    /// MP7 engine on / off.
    Mp7,
    /// The name of an MP7 instrument.
    Mp7Inst,
    /// Two-sided drift: 50 % = none, below = late, above = early.
    Drift,
    /// -1..1 = half a step early .. half a step late.
    Nudge,
    /// 0..1 = straight .. 75 % swing.
    Swing,
    Accent,
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
            Show::Accent => format!("+{:.1} dB", 20.0 * (accent_gain(plain)).log10()),
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
            Show::Mp7 => (if plain >= 0.5 { "On (MP7 sound)" } else { "Off" }).to_string(),
            Show::Mp7Inst => crate::mp7::INSTRUMENT_NAMES[crate::mp7::instrument_index(plain)].to_string(),
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

/// Kits offered by the KIT selector: 0 = MINI (the first-day sounds), 1..=16 = factory kits,
/// 17 = MP7 (Mini Pops 7 style instruments).
pub const NUM_KIT_CHOICES: usize = NUM_KITS + 2;

pub fn kit_choice(i: usize) -> &'static Kit {
    if i == 0 {
        &MINI_KIT
    } else if i == MP7_KIT_CHOICE {
        &MP7_KIT
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
        // +4 dB and a little retrigger heat at 100 %; 50 % by default
        ParamId::Accent => Spec {
            name: "Accent",
            short: "ACCENT",
            min: 0.0,
            max: 1.0,
            default: 0.5,
            curve: Curve::Linear,
            show: Show::Accent,
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
        ParamId::Mp7 => Spec {
            name: "MP7 Sound",
            short: "MP7",
            min: 0.0,
            max: 1.0,
            default: 0.0,
            curve: Curve::Toggle,
            show: Show::Mp7,
        },
        ParamId::Mp7Inst(c) => Spec {
            name: "MP7 Instrument",
            short: "INSTR",
            min: 0.0,
            max: (crate::mp7::NUM_INSTRUMENTS - 1) as f32,
            default: MP7_INSTRUMENTS[c.min(NUM_CH - 1)] as f32,
            curve: Curve::Linear,
            show: Show::Mp7Inst,
        },
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
        PhDepth => knob("Phaser Depth", "DEPTH", 0.75, Show::Percent),
        PhColor => knob("Phaser Color", "COLOR", 0.45, Show::Percent),
        PhMix => knob("Phaser Mix", "MIX", 1.0, Show::Percent),
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

/// Load a kit choice (0 = MINI, 1..=16 = factory kits, 17 = MP7) into the parameters: sounds,
/// volumes, pans, master, tempo, and the matching sound engine (and the MP7 instruments).
pub fn apply_kit(access: &dyn ParamAccess, kit: usize) {
    let k = kit_choice(kit);
    let mp7 = kit == MP7_KIT_CHOICE;
    access.set_gesture(ParamId::Sound, if kit == 0 || mp7 { 0.0 } else { 1.0 });
    access.set_gesture(ParamId::Mp7, if mp7 { 1.0 } else { 0.0 });
    if mp7 {
        for c in 0..NUM_CH {
            access.set_gesture(ParamId::Mp7Inst(c), MP7_INSTRUMENTS[c] as f32);
        }
    }
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

/// Level of an accented hit for the ACCENT knob (0..1): x1 to x1.6 (+4 dB).
pub fn accent_gain(knob: f32) -> f32 {
    1.0 + 0.6 * knob.clamp(0.0, 1.0)
}

/// The engine a set of parameters plays (MP7 takes the place of MINI / ORIGINAL when on).
pub fn sound_engine(access: &dyn ParamAccess) -> crate::dsp::SoundEngine {
    use crate::dsp::SoundEngine;
    if access.get(ParamId::Mp7) >= 0.5 {
        SoundEngine::Mp7
    } else if access.get(ParamId::Sound) >= 0.5 {
        SoundEngine::Original
    } else {
        SoundEngine::Mini
    }
}

/// Choose the sound engine from the panel. Turning MP7 on gives every voice the sound of the
/// MP7 instrument it is set to (the knobs mean other things on MP7); the mixer stays as it is.
pub fn set_sound_engine(access: &dyn ParamAccess, engine: crate::dsp::SoundEngine) {
    use crate::dsp::SoundEngine;
    let was_mp7 = access.get(ParamId::Mp7) >= 0.5;
    match engine {
        SoundEngine::Mini => {
            access.set_gesture(ParamId::Mp7, 0.0);
            access.set_gesture(ParamId::Sound, 0.0);
        }
        SoundEngine::Original => {
            access.set_gesture(ParamId::Mp7, 0.0);
            access.set_gesture(ParamId::Sound, 1.0);
        }
        SoundEngine::Mp7 => {
            access.set_gesture(ParamId::Mp7, 1.0);
            if !was_mp7 {
                for c in 0..NUM_CH {
                    let inst = crate::mp7::instrument_index(access.get(ParamId::Mp7Inst(c)));
                    load_mp7_instrument(access, c, inst);
                }
            }
        }
    }
}

/// Give voice `c` an MP7 instrument with its preset sound.
pub fn load_mp7_instrument(access: &dyn ParamAccess, c: usize, inst: usize) {
    let inst = inst.min(crate::mp7::NUM_INSTRUMENTS - 1);
    access.set_gesture(ParamId::Mp7Inst(c), inst as f32);
    for (i, v) in crate::mp7::preset(inst).iter().enumerate() {
        access.set_gesture(ParamId::Voice(c, VOICE_PARAMS[i]), *v);
    }
}

#[cfg(test)]
mod mp7_param_tests {
    use super::*;
    use crate::dsp::SoundEngine;
    use std::sync::Mutex;

    /// Plain parameter memory for tests.
    struct Mem(Mutex<Vec<f32>>);
    impl Mem {
        fn new() -> Self {
            let mut v = vec![0.0; NUM_PARAMS];
            for id in ParamId::all() {
                v[id.index()] = spec(id).default;
            }
            Mem(Mutex::new(v))
        }
    }
    impl ParamAccess for Mem {
        fn get(&self, id: ParamId) -> f32 {
            self.0.lock().unwrap()[id.index()]
        }
        fn begin(&self, _: ParamId) {}
        fn set(&self, id: ParamId, plain: f32) {
            let s = spec(id);
            self.0.lock().unwrap()[id.index()] = plain.clamp(s.min, s.max);
        }
        fn end(&self, _: ParamId) {}
    }

    #[test]
    fn every_parameter_has_its_own_slot() {
        let all = ParamId::all();
        assert_eq!(all.len(), NUM_PARAMS);
        let mut seen = vec![false; NUM_PARAMS];
        for id in all {
            assert!(!seen[id.index()], "{id:?} shares a slot");
            seen[id.index()] = true;
        }
        // the 2.3 parameters keep their numbers: MP7 comes after ACCENT
        assert_eq!(ParamId::Accent.index(), 176);
        assert_eq!(ParamId::Mp7.index(), 177);
    }

    #[test]
    fn mp7_kit_turns_the_mp7_engine_on_and_other_kits_turn_it_off() {
        let m = Mem::new();
        assert_eq!(sound_engine(&m), SoundEngine::Mini, "MINI by default");
        apply_kit(&m, MP7_KIT_CHOICE);
        assert_eq!(sound_engine(&m), SoundEngine::Mp7);
        assert_eq!(crate::mp7::instrument_index(m.get(ParamId::Mp7Inst(5))), crate::mp7::QUIJADA);
        assert!((m.get(ParamId::Voice(5, VoiceParam::ToneTune)) - MP7_KIT.voices[5][0]).abs() < 1e-6);
        apply_kit(&m, 3);
        assert_eq!(sound_engine(&m), SoundEngine::Original);
        apply_kit(&m, 0);
        assert_eq!(sound_engine(&m), SoundEngine::Mini);
    }

    #[test]
    fn switching_to_mp7_gives_each_voice_its_instrument_sound() {
        let m = Mem::new();
        m.set(ParamId::Mp7Inst(2), crate::mp7::GUIRO as f32);
        set_sound_engine(&m, SoundEngine::Mp7);
        assert_eq!(sound_engine(&m), SoundEngine::Mp7);
        let p = crate::mp7::preset(crate::mp7::GUIRO);
        for (i, vp) in VOICE_PARAMS.iter().enumerate().take(13) {
            assert!((m.get(ParamId::Voice(2, *vp)) - p[i]).abs() < 1e-6);
        }
        // back to ORIGINAL keeps the knobs; MP7 off
        set_sound_engine(&m, SoundEngine::Original);
        assert_eq!(sound_engine(&m), SoundEngine::Original);
        assert!((m.get(ParamId::Voice(2, VoiceParam::ToneTune)) - p[0]).abs() < 1e-6);
    }

    #[test]
    fn banks_from_2_3_open_without_mp7() {
        let m = Mem::new();
        apply_kit(&m, MP7_KIT_CHOICE);
        let state = crate::state::SharedState::default();
        let mut text = crate::bank::bank_to_json(&m, &state);
        assert!(text.contains("\"mp7\"") && text.contains("\"f_mp7_instrument\""));
        // a bank saved by 2.3: no MP7 keys at all
        let mut f: crate::bank::BankFile = serde_json::from_str(&text).unwrap();
        f.params.retain(|k, _| !k.contains("mp7"));
        text = serde_json::to_string(&f).unwrap();
        crate::bank::bank_from_json(&text, &m, &state).unwrap();
        assert_eq!(sound_engine(&m), SoundEngine::Mini, "a 2.3 bank must play as it did");
    }

    #[test]
    fn mp7_sound_plays_through_the_engine() {
        use crate::engine::{Engine, Transport, Values};
        let m = Mem::new();
        apply_kit(&m, MP7_KIT_CHOICE);
        let vals = Values::read(|id| m.get(id));
        assert_eq!(vals.engine, SoundEngine::Mp7);
        let state = crate::state::SharedState::default();
        let mut e = Engine::new(48000.0);
        e.note_on(&state, 0, crate::engine::BASE_NOTE + 5, 1.0); // F: quijada
        e.begin_block(&state, &vals, &Transport::default());
        let mut peak = 0.0f32;
        for _ in 0..24000 {
            let f = e.next_frame(&state, &vals);
            assert!(f.left.is_finite());
            peak = peak.max(f.voices[5].abs());
        }
        assert!(peak > 0.05 && peak < 2.0, "quijada through the engine: {peak}");
    }
}
