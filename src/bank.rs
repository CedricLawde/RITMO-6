//! Bank (all programs + modules + mixer) and single-module files, stored as JSON.

use crate::params::{ParamAccess, ParamId, VoiceParam, NUM_CH, VOICE_LETTERS, VOICE_PARAMS};
use crate::state::{PatternData, SharedState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One rhythm program: 6 rows of 16 steps, MULTIPLY hit counts and length.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct ProgramPattern {
    pub rows: [u16; NUM_CH],
    #[serde(default)]
    pub multi: [u32; NUM_CH],
    pub length: u8,
}

#[derive(Serialize, Deserialize)]
pub struct PatternFile {
    pub format: String,
    pub pattern: ProgramPattern,
}

#[derive(Serialize, Deserialize)]
pub struct PatternSetFile {
    pub format: String,
    pub programs: Vec<ProgramPattern>,
}

fn read_program(state: &SharedState, prog: usize) -> ProgramPattern {
    use std::sync::atomic::Ordering::Relaxed;
    ProgramPattern {
        rows: std::array::from_fn(|c| state.steps[prog][c].load(Relaxed)),
        multi: std::array::from_fn(|c| state.multi[prog][c].load(Relaxed)),
        length: state.length(prog) as u8,
    }
}

fn write_program(state: &SharedState, prog: usize, p: &ProgramPattern) {
    use std::sync::atomic::Ordering::Relaxed;
    for c in 0..NUM_CH {
        state.steps[prog][c].store(p.rows[c], Relaxed);
        state.multi[prog][c].store(p.multi[c], Relaxed);
    }
    state.lengths[prog].store(p.length.clamp(1, 16), Relaxed);
}

/// SAVE PRG: the current program alone (*.r6p).
pub fn save_program_dialog(state: &SharedState, prog: usize) -> Option<String> {
    let path = rfd::FileDialog::new()
        .add_filter("Ritmo-6 pattern", &["r6p"])
        .set_file_name(format!("pattern-{:02}.r6p", prog + 1))
        .save_file()?;
    let f = PatternFile { format: "ritmo6-pattern-1".into(), pattern: read_program(state, prog) };
    Some(match std::fs::write(&path, serde_json::to_string_pretty(&f).unwrap_or_default()) {
        Ok(_) => format!("Program {} saved", prog + 1),
        Err(e) => format!("Save failed: {e}"),
    })
}

/// LOAD PRG: load a pattern file into the current program.
pub fn load_program_dialog(state: &SharedState, prog: usize) -> Option<String> {
    let path = rfd::FileDialog::new().add_filter("Ritmo-6 pattern", &["r6p"]).pick_file()?;
    let text = std::fs::read_to_string(&path).ok()?;
    Some(match serde_json::from_str::<PatternFile>(&text) {
        Ok(f) => {
            write_program(state, prog, &f.pattern);
            format!("Pattern loaded into program {}", prog + 1)
        }
        Err(e) => format!("Not a pattern file: {e}"),
    })
}

/// SAVE ALL: the 16 programs (*.r6s), without the sounds.
pub fn save_patterns_dialog(state: &SharedState) -> Option<String> {
    let path = rfd::FileDialog::new()
        .add_filter("Ritmo-6 pattern set", &["r6s"])
        .set_file_name("patterns.r6s")
        .save_file()?;
    let f = PatternSetFile {
        format: "ritmo6-patterns-1".into(),
        programs: (0..crate::params::NUM_PROGS).map(|p| read_program(state, p)).collect(),
    };
    Some(match std::fs::write(&path, serde_json::to_string_pretty(&f).unwrap_or_default()) {
        Ok(_) => "16 programs saved".into(),
        Err(e) => format!("Save failed: {e}"),
    })
}

/// LOAD ALL: replace the 16 programs, keeping the sounds.
pub fn load_patterns_dialog(state: &SharedState) -> Option<String> {
    let path = rfd::FileDialog::new().add_filter("Ritmo-6 pattern set", &["r6s"]).pick_file()?;
    let text = std::fs::read_to_string(&path).ok()?;
    Some(match serde_json::from_str::<PatternSetFile>(&text) {
        Ok(f) => {
            for (p, pat) in f.programs.iter().enumerate().take(crate::params::NUM_PROGS) {
                write_program(state, p, pat);
            }
            format!("{} programs loaded", f.programs.len().min(crate::params::NUM_PROGS))
        }
        Err(e) => format!("Not a pattern set: {e}"),
    })
}

#[derive(Serialize, Deserialize)]
pub struct BankFile {
    pub format: String,
    pub patterns: PatternData,
    pub params: BTreeMap<String, f32>,
}

#[derive(Serialize, Deserialize)]
pub struct ModuleFile {
    pub format: String,
    pub values: BTreeMap<String, f32>,
}

fn voice_key(p: VoiceParam) -> &'static str {
    match p {
        VoiceParam::ToneTune => "tone_tune",
        VoiceParam::ToneDecay => "tone_decay",
        VoiceParam::ToneVolume => "tone_volume",
        VoiceParam::PitchDecay => "pitch_decay",
        VoiceParam::PitchDecayVol => "pitch_decay_vol",
        VoiceParam::PhaseModRate => "phase_mod_rate",
        VoiceParam::PhaseModLev => "phase_mod_level",
        VoiceParam::NoiseVolume => "noise_volume",
        VoiceParam::NoiseDecay => "noise_decay",
        VoiceParam::FilterFreq => "filter_frequency",
        VoiceParam::FilterReso => "filter_resonance",
        VoiceParam::FilterDecay => "filter_decay",
        VoiceParam::FilterHp => "filter_lp_hp",
        VoiceParam::PanRandom => "pan_random",
        VoiceParam::Nudge => "nudge",
        VoiceParam::DriftAmount => "drift_amount",
    }
}

pub fn param_key(id: ParamId) -> String {
    let l = |c: usize| VOICE_LETTERS[c].to_lowercase();
    match id {
        ParamId::Level(c) => format!("level_{}", l(c)),
        ParamId::Pan(c) => format!("pan_{}", l(c)),
        ParamId::Mute(c) => format!("mute_{}", l(c)),
        ParamId::Master => "master".into(),
        ParamId::Tempo => "tempo".into(),
        ParamId::Sync => "sync".into(),
        ParamId::Sound => "sound".into(),
        ParamId::TempoDrift => "tempo_drift".into(),
        ParamId::SoundDrift => "sound_drift".into(),
        ParamId::Swing => "swing".into(),
        ParamId::Voice(c, p) => format!("{}_{}", l(c), voice_key(p)),
        ParamId::Send(c, k) => format!("{}_send_{}", l(c), crate::params::FX_BUS_NAMES[k].to_lowercase()),
        ParamId::Fx(p) => format!("fx_{}", format!("{p:?}").to_lowercase()),
    }
}

pub fn bank_to_json(access: &dyn ParamAccess, state: &SharedState) -> String {
    let params = ParamId::all().into_iter().map(|id| (param_key(id), access.get(id))).collect();
    let f = BankFile { format: "ritmo6-bank-1".into(), patterns: state.snapshot(), params };
    serde_json::to_string_pretty(&f).unwrap_or_default()
}

pub fn bank_from_json(text: &str, access: &dyn ParamAccess, state: &SharedState) -> Result<(), String> {
    let f: BankFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
    state.restore(&f.patterns);
    for id in ParamId::all() {
        if let Some(v) = f.params.get(&param_key(id)) {
            access.set_gesture(id, *v);
        } else if id.is_fx() {
            // banks saved before the FX expansion: sends at zero and FX at their defaults,
            // so the bank sounds exactly as it did
            access.set_gesture(id, crate::params::spec(id).default);
        }
    }
    Ok(())
}

pub fn module_to_json(access: &dyn ParamAccess, ch: usize) -> String {
    let values = VOICE_PARAMS
        .iter()
        .map(|p| (voice_key(*p).to_string(), access.get(ParamId::Voice(ch, *p))))
        .collect();
    let f = ModuleFile { format: "ritmo6-module-1".into(), values };
    serde_json::to_string_pretty(&f).unwrap_or_default()
}

pub fn module_from_json(text: &str, access: &dyn ParamAccess, ch: usize) -> Result<(), String> {
    let f: ModuleFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
    for p in VOICE_PARAMS {
        if let Some(v) = f.values.get(voice_key(p)) {
            access.set_gesture(ParamId::Voice(ch.min(NUM_CH - 1), p), *v);
        }
    }
    Ok(())
}

fn dialog(kind: &str) -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter(kind, &["json"])
}

pub fn save_bank_dialog(access: &dyn ParamAccess, state: &SharedState) -> Option<String> {
    let path = dialog("Ritmo-6 bank").set_file_name("bank.json").save_file()?;
    match std::fs::write(&path, bank_to_json(access, state)) {
        Ok(_) => Some("Bank saved".into()),
        Err(e) => Some(format!("Save failed: {e}")),
    }
}

pub fn load_bank_dialog(access: &dyn ParamAccess, state: &SharedState) -> Option<String> {
    let path = dialog("Ritmo-6 bank").pick_file()?;
    let text = std::fs::read_to_string(&path).ok()?;
    Some(match bank_from_json(&text, access, state) {
        Ok(_) => "Bank loaded".into(),
        Err(e) => format!("Not a bank file: {e}"),
    })
}

pub fn save_module_dialog(access: &dyn ParamAccess, ch: usize) -> Option<String> {
    let name = format!("module_{}.json", VOICE_LETTERS[ch]);
    let path = dialog("Ritmo-6 module").set_file_name(&name).save_file()?;
    match std::fs::write(&path, module_to_json(access, ch)) {
        Ok(_) => Some("Module saved".into()),
        Err(e) => Some(format!("Save failed: {e}")),
    }
}

pub fn load_module_dialog(access: &dyn ParamAccess, ch: usize) -> Option<String> {
    let path = dialog("Ritmo-6 module").pick_file()?;
    let text = std::fs::read_to_string(&path).ok()?;
    Some(match module_from_json(&text, access, ch) {
        Ok(_) => "Module loaded".into(),
        Err(e) => format!("Not a module file: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering::Relaxed;

    #[test]
    fn pattern_round_trip_keeps_steps_multiply_and_length() {
        let a = SharedState::default();
        a.cycle_hits(3, 2, 5); // on, 1 hit
        a.cycle_hits(3, 2, 5); // 2 hits
        a.cycle_hits(3, 2, 5); // 3 hits
        a.lengths[3].store(12, Relaxed);
        let set = PatternSetFile {
            format: "ritmo6-patterns-1".into(),
            programs: (0..crate::params::NUM_PROGS).map(|p| read_program(&a, p)).collect(),
        };
        let text = serde_json::to_string(&set).unwrap();
        let back: PatternSetFile = serde_json::from_str(&text).unwrap();
        let b = SharedState::default();
        for c in 0..NUM_CH {
            b.steps[3][c].store(0, Relaxed);
        }
        for (p, pat) in back.programs.iter().enumerate() {
            write_program(&b, p, pat);
        }
        assert_eq!(b.hits_at(3, 2, 5), 3);
        assert_eq!(b.length(3), 12);
        for p in 0..crate::params::NUM_PROGS {
            for c in 0..NUM_CH {
                assert_eq!(a.steps[p][c].load(Relaxed), b.steps[p][c].load(Relaxed));
                assert_eq!(a.multi[p][c].load(Relaxed), b.multi[p][c].load(Relaxed));
            }
        }
    }
}
