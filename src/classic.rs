//! MINI sound: the rebuild's first engine, kept because the person prefers its character
//! (exponential decays, pitch sweep, sine-to-square shape, noise through a TPT state-variable
//! filter). It reads the same knobs as the original model:
//! Tune -> pitch, Pitch Decay Vol -> sweep depth (x4.8 semitones), Pitch Decay -> sweep time,
//! Phase Mod Level -> shape (sine .. triangle .. square), every decay -> time to -60 dB.

use crate::dsp::VoiceSettings;
use crate::params::{env_seconds, volts_to_hz, FILTER_HP_THRESHOLD};

#[derive(Clone, Copy, Default)]
pub struct Classic {
    phase: f32,
    amp: f32,
    pitch_env: f32,
    noise_env: f32,
    ic1: f32,
    ic2: f32,
    held: f32,
    declick: f32,
    last: f32,
    active: bool,
}

/// Per-sample coefficient reaching -60 dB after `seconds`.
fn decay_coef(seconds: f32, sr: f32) -> f32 {
    let samples = (seconds.max(0.0005) * sr).max(1.0);
    (-6.907_755 / samples).exp()
}

impl Classic {
    pub fn trigger(&mut self) {
        self.held = self.last;
        self.declick = 1.0;
        self.phase = 0.0;
        self.amp = 1.0;
        self.pitch_env = 1.0;
        self.noise_env = 1.0;
        self.active = true;
    }

    pub fn render(&mut self, s: &VoiceSettings, sr: f32, noise: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let sweep_st = s.pitch_decay_vol * 4.8;
        let freq = (volts_to_hz(s.tone_tune) * (2.0f32).powf(sweep_st * self.pitch_env / 12.0)).min(sr * 0.45);
        self.phase = (self.phase + freq / sr).fract();
        let ph = self.phase;
        let sine = (ph * std::f32::consts::TAU).sin();
        let tri = if ph < 0.25 {
            ph * 4.0
        } else if ph < 0.75 {
            2.0 - ph * 4.0
        } else {
            ph * 4.0 - 4.0
        };
        let square = (sine * 6.0).clamp(-1.0, 1.0);
        let shape = (s.pm_level / 10.0).clamp(0.0, 1.0);
        let osc = if shape < 0.5 {
            sine + (tri - sine) * (shape * 2.0)
        } else {
            tri + (square - tri) * ((shape - 0.5) * 2.0)
        };
        let tone_out = osc * self.amp * (s.tone_volume / 10.0);

        let fc = volts_to_hz(s.filter_freq).clamp(20.0, sr * 0.45);
        let g = (std::f32::consts::PI * fc / sr).tan();
        let k = 2.0 - 1.94 * (s.filter_reso / 10.0).clamp(0.0, 1.0);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = noise - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        let filtered = if s.filter_hp >= FILTER_HP_THRESHOLD { noise - k * v1 - v2 } else { v2 };
        let noise_out = filtered * self.noise_env * (s.noise_volume / 10.0);

        let mut out = tone_out + noise_out;
        if self.declick > 0.0 {
            out += self.held * self.declick;
            self.declick = (self.declick - 1.0 / (0.002 * sr)).max(0.0);
        }
        self.last = out;
        self.amp *= decay_coef(env_seconds(s.tone_decay), sr);
        self.noise_env *= decay_coef(env_seconds(s.noise_decay), sr);
        self.pitch_env *= decay_coef(env_seconds(s.pitch_decay), sr);
        if self.amp < 1e-5 && self.noise_env < 1e-5 {
            self.active = false;
        }
        out
    }
}
