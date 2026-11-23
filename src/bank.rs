//! Bank (all programs + modules + mixer) and single-module files, stored as JSON.

use crate::params::{ParamAccess, ParamId, VoiceParam, NUM_CH, VOICE_LETTERS, VOICE_PARAMS};
use crate::state::{PatternData, SharedState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One rhythm program: 6 rows of 16 steps, MULTIPLY hit counts, ACCENT, PROB and lengths.
/// The v2 fields are optional, so v1 pattern files still load (no accent, 100 %, one length).
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct ProgramPattern {
    pub rows: [u16; NUM_CH],
    #[serde(default)]
    pub multi: [u32; NUM_CH],
    pub length: u8,
    #[serde(default)]
    pub accent: [u16; NUM_CH],
    #[serde(default)]
    pub prob: [u32; NUM_CH],
    #[serde(default)]
    pub voice_lengths: Option<[u8; NUM_CH]>,
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

pub fn read_program(state: &SharedState, prog: usize) -> ProgramPattern {
    use std::sync::atomic::Ordering::Relaxed;
    ProgramPattern {
        rows: std::array::from_fn(|c| state.steps[prog][c].load(Relaxed)),
        multi: std::array::from_fn(|c| state.multi[prog][c].load(Relaxed)),
        length: state.length(prog) as u8,
        accent: std::array::from_fn(|c| state.accent[prog][c].load(Relaxed)),
        prob: std::array::from_fn(|c| state.prob[prog][c].load(Relaxed)),
        voice_lengths: Some(std::array::from_fn(|c| state.voice_length(prog, c) as u8)),
    }
}

pub fn write_program(state: &SharedState, prog: usize, p: &ProgramPattern) {
    use std::sync::atomic::Ordering::Relaxed;
    for c in 0..NUM_CH {
        state.steps[prog][c].store(p.rows[c], Relaxed);
        state.multi[prog][c].store(p.multi[c], Relaxed);
        state.accent[prog][c].store(p.accent[c], Relaxed);
        state.prob[prog][c].store(p.prob[c], Relaxed);
    }
    state.set_length(prog, p.length as usize);
    if let Some(vl) = p.voice_lengths {
        for c in 0..NUM_CH {
            state.voice_lengths[prog][c].store(vl[c].clamp(1, 16), Relaxed);
        }
    }
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
        ParamId::Accent => "accent".into(),
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
        } else if id.is_fx() || id == ParamId::Accent {
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

// ------------------------------------------------------------------ file dialogs off the GUI thread
//
// A Windows file dialog runs its own message loop until it closes. Opened from inside the plugin
// window's drawing (as 1.0-1.3.4 did), that loop sends messages back into the plugin window while
// it is still busy drawing, the window code trips over itself and the plugin stops the host
// (crash 0xc0000409 in Reaper, Cubase, VSTHost on LOAD / SAVE). The dialogs now run on their own
// thread; the panel keeps drawing, and the result is picked up on a later frame.

/// What a file dialog came back with.
pub enum DialogDone {
    /// A message for the panel (saved, cancelled, failed).
    Message(String),
    /// A bank file's text, to apply on the GUI thread.
    Bank(String),
    /// A module file's text for voice `ch`.
    Module(usize, String),
}

/// A file dialog running on its own thread.
/// Empty while the window is open, then `Some(result)`; the result is `None` when the person
/// cancelled. (A shared slot rather than a channel: the plugin's panel state must be `Sync`.)
pub type DialogTask = std::sync::Arc<std::sync::Mutex<Option<Option<DialogDone>>>>;

fn spawn(job: impl FnOnce() -> Option<DialogDone> + Send + 'static) -> DialogTask {
    let slot: DialogTask = Default::default();
    let out = slot.clone();
    let started = std::thread::Builder::new().name("ritmo6-file-dialog".into()).spawn(move || {
        let done = job();
        if let Ok(mut s) = out.lock() {
            *s = Some(done);
        }
    });
    if let Err(e) = started {
        if let Ok(mut s) = slot.lock() {
            *s = Some(Some(DialogDone::Message(format!("Could not open the file window: {e}"))));
        }
    }
    slot
}

/// SAVE: the bank is written to text now (on the GUI thread), the dialog and the file writing
/// happen on the dialog thread.
pub fn save_bank_task(access: &dyn ParamAccess, state: &SharedState) -> DialogTask {
    let text = bank_to_json(access, state);
    spawn(move || {
        let path = dialog("Ritmo-6 bank").set_file_name("bank.json").save_file()?;
        Some(DialogDone::Message(match std::fs::write(&path, text) {
            Ok(_) => "Bank saved".into(),
            Err(e) => format!("Save failed: {e}"),
        }))
    })
}

/// LOAD: pick and read the file on the dialog thread; the panel applies it.
pub fn load_bank_task() -> DialogTask {
    spawn(|| {
        let path = dialog("Ritmo-6 bank").pick_file()?;
        Some(match std::fs::read_to_string(&path) {
            Ok(text) => DialogDone::Bank(text),
            Err(e) => DialogDone::Message(format!("Could not read the file: {e}")),
        })
    })
}

pub fn save_module_task(access: &dyn ParamAccess, ch: usize) -> DialogTask {
    let text = module_to_json(access, ch);
    let name = format!("module_{}.json", VOICE_LETTERS[ch.min(NUM_CH - 1)]);
    spawn(move || {
        let path = dialog("Ritmo-6 module").set_file_name(&name).save_file()?;
        Some(DialogDone::Message(match std::fs::write(&path, text) {
            Ok(_) => "Module saved".into(),
            Err(e) => format!("Save failed: {e}"),
        }))
    })
}

pub fn load_module_task(ch: usize) -> DialogTask {
    spawn(move || {
        let path = dialog("Ritmo-6 module").pick_file()?;
        Some(match std::fs::read_to_string(&path) {
            Ok(text) => DialogDone::Module(ch, text),
            Err(e) => DialogDone::Message(format!("Could not read the file: {e}")),
        })
    })
}

/// Apply a finished dialog on the GUI thread; returns the message to show.
pub fn finish_dialog(done: DialogDone, access: &dyn ParamAccess, state: &SharedState) -> String {
    match done {
        DialogDone::Message(m) => m,
        DialogDone::Bank(text) => match bank_from_json(&text, access, state) {
            Ok(_) => "Bank loaded".into(),
            Err(e) => format!("Not a bank file: {e}"),
        },
        DialogDone::Module(ch, text) => match module_from_json(&text, access, ch) {
            Ok(_) => "Module loaded".into(),
            Err(e) => format!("Not a module file: {e}"),
        },
    }
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
    fn file_jobs_finish_off_the_gui_thread() {
        let gui = std::thread::current().id();
        let task = spawn(move || {
            assert_ne!(std::thread::current().id(), gui, "the file window must not run on the GUI thread");
            Some(DialogDone::Message("done".into()))
        });
        let cancelled = spawn(|| None);
        let wait = |t: &DialogTask| {
            for _ in 0..200 {
                if let Some(r) = t.lock().unwrap().take() {
                    return Some(r);
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            None
        };
        assert!(matches!(wait(&task), Some(Some(DialogDone::Message(m))) if m == "done"));
        assert!(matches!(wait(&cancelled), Some(None)));
    }

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

    #[test]
    fn v1_files_still_load() {
        // a v1 pattern file: no accent, no chance, one length
        let v1 = r#"{"format":"ritmo6-pattern-1","pattern":{"rows":[1,2,3,4,5,6],"multi":[0,0,0,0,0,0],"length":12}}"#;
        let f: PatternFile = serde_json::from_str(v1).unwrap();
        let s = SharedState::default();
        write_program(&s, 2, &f.pattern);
        assert_eq!(s.length(2), 12);
        assert!((0..NUM_CH).all(|c| s.voice_length(2, c) == 12));
        assert!((0..16).all(|st| s.prob_at(2, 0, st) == 100 && !s.accent_on(2, 0, st)));
        // a v1 bank's pattern memory with the MULTIPLY switch on
        let d: PatternData = serde_json::from_str(
            r#"{"steps":[[1,0,0,0,0,0]],"lengths":[10],"program":0,"midi_channel":0,"keyb_ctrl":true,"multiply_mode":true}"#,
        )
        .unwrap();
        s.restore(&d);
        assert_eq!(s.voice_length(0, 4), 10);
        assert_eq!(s.pad_mode.load(Relaxed), crate::state::PAD_MULTI);
    }

    #[test]
    fn v2_fields_round_trip() {
        let a = SharedState::default();
        a.toggle_accent(5, 1, 3);
        a.cycle_prob(5, 2, 7);
        a.cycle_prob(5, 2, 7); // 75 %
        a.set_voice_length(5, 4, 7);
        a.pad_mode.store(crate::state::PAD_PROB, Relaxed);
        let text = serde_json::to_string(&a.snapshot()).unwrap();
        let b = SharedState::default();
        b.restore(&serde_json::from_str(&text).unwrap());
        assert!(b.accent_on(5, 1, 3));
        assert_eq!(b.prob_at(5, 2, 7), 75);
        assert_eq!(b.voice_length(5, 4), 7);
        assert_eq!(b.pad_mode.load(Relaxed), crate::state::PAD_PROB);
        // one program through a pattern file (COPY / PASTE and SAVE PRG use the same record)
        let p: ProgramPattern = serde_json::from_str(&serde_json::to_string(&read_program(&a, 5)).unwrap()).unwrap();
        let c = SharedState::default();
        write_program(&c, 0, &p);
        assert!(c.accent_on(0, 1, 3));
        assert_eq!(c.prob_at(0, 2, 7), 75);
        assert_eq!(c.voice_length(0, 4), 7);
        assert_eq!(c.voice_length(0, 0), 16);
    }
}

