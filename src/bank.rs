//! Bank (all programs + modules + mixer) and single-module files, stored as JSON.

use crate::params::{ParamAccess, ParamId, VoiceParam, NUM_CH, VOICE_LETTERS, VOICE_PARAMS};
use crate::state::{PatternData, SharedState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
        ParamId::Voice(c, p) => format!("{}_{}", l(c), voice_key(p)),
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
