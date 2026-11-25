//! Ritmo-6 as a VST3 (and CLAP) instrument: stereo mix plus one output per voice.

use nih_plug::params::persist::PersistentField;
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui, EguiState};


use ritmo6_core::engine::{Engine, Transport, Values};
use ritmo6_core::params::{self as rp, ParamAccess, ParamId, VoiceParam, NUM_CH, VOICE_LETTERS};
use ritmo6_core::state::{PatternData, SharedState};
use ritmo6_core::setup::SetupHost;
use ritmo6_core::ui;
use std::sync::Arc;

/// Saves the 16 programs (and MIDI settings) with the host project.
pub struct PatternStore(Arc<SharedState>);

impl<'a> PersistentField<'a, PatternData> for PatternStore {
    fn set(&self, new_value: PatternData) {
        self.0.restore(&new_value);
    }
    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&PatternData) -> R,
    {
        f(&self.0.snapshot())
    }
}

fn fparam(id: ParamId) -> FloatParam {
    let s = rp::spec(id);
    let name = match id {
        ParamId::Level(c) | ParamId::Pan(c) | ParamId::Voice(c, _) | ParamId::Send(c, _) => {
            format!("{} {}", VOICE_LETTERS[c], s.name)
        }
        _ => s.name.to_string(),
    };
    let range = FloatRange::Linear { min: s.min, max: s.max };
    FloatParam::new(name, s.default, range)
        .with_value_to_string(Arc::new(move |v| rp::spec(id).format(v)))
}

fn bparam(id: ParamId) -> BoolParam {
    let s = rp::spec(id);
    let name = match id {
        ParamId::Mute(c) | ParamId::Voice(c, _) => format!("{} {}", VOICE_LETTERS[c], s.name),
        _ => s.name.to_string(),
    };
    BoolParam::new(name, s.default >= 0.5)
}

/// The noise filter's LP/HP switch of one voice: off = low-pass, on = high-pass.
fn lphp_param(c: usize) -> BoolParam {
    let s = rp::spec(ParamId::Voice(c, VoiceParam::FilterHp));
    BoolParam::new(format!("{} {}", VOICE_LETTERS[c], s.name), s.default >= rp::FILTER_HP_THRESHOLD)
        .with_value_to_string(Arc::new(|hp| (if hp { "High-pass" } else { "Low-pass" }).to_string()))
        .with_string_to_value(Arc::new(|t| {
            let t = t.trim().to_lowercase();
            if t.starts_with('h') || t == "1" || t == "on" {
                Some(true)
            } else if t.starts_with('l') || t == "0" || t == "off" {
                Some(false)
            } else {
                None
            }
        }))
}

/// Projects saved before 2.1 hold LP/HP as a number 0-10 (5 and up = high-pass): turn it into
/// the switch's on/off so they open with the same filter.
fn convert_old_lphp(params: &mut std::collections::BTreeMap<String, nih_plug::wrapper::state::ParamValue>) {
    use nih_plug::wrapper::state::ParamValue;
    for (id, value) in params.iter_mut() {
        if id == "flphp" || id.starts_with("flphp_") {
            if let ParamValue::F32(v) = *value {
                *value = ParamValue::Bool(v >= rp::FILTER_HP_THRESHOLD);
            }
        }
    }
}

#[derive(Params)]
pub struct ChannelParams {
    #[id = "level"]
    pub level: FloatParam,
    #[id = "pan"]
    pub pan: FloatParam,
    #[id = "mute"]
    pub mute: BoolParam,
    #[id = "tune"]
    pub tone_tune: FloatParam,
    #[id = "tdecay"]
    pub tone_decay: FloatParam,
    #[id = "tvol"]
    pub tone_volume: FloatParam,
    #[id = "pdecay"]
    pub pitch_decay: FloatParam,
    #[id = "pdvol"]
    pub pitch_decay_vol: FloatParam,
    #[id = "pmrate"]
    pub pm_rate: FloatParam,
    #[id = "pmlev"]
    pub pm_level: FloatParam,
    #[id = "nvol"]
    pub noise_volume: FloatParam,
    #[id = "ndecay"]
    pub noise_decay: FloatParam,
    #[id = "ffreq"]
    pub filter_freq: FloatParam,
    #[id = "freso"]
    pub filter_reso: FloatParam,
    #[id = "fdecay"]
    pub filter_decay: FloatParam,
    /// LP/HP is a two-position switch: declared to the host as one (it was a 0-10 knob before
    /// 2.1, which Cubase's VST3 could get out of step with after a module LOAD). Same id, so
    /// automation stays attached; older projects are converted in `filter_state`.
    #[id = "flphp"]
    pub filter_hp: BoolParam,
    #[id = "panrnd"]
    pub pan_random: FloatParam,
    #[id = "nudge"]
    pub nudge: FloatParam,
    #[id = "drshare"]
    pub drift_amount: FloatParam,
    #[id = "sph"]
    pub send_phaser: FloatParam,
    #[id = "sds"]
    pub send_dist: FloatParam,
    #[id = "sdl"]
    pub send_delay: FloatParam,
    #[id = "ssp"]
    pub send_spring: FloatParam,
}

impl ChannelParams {
    fn new(c: usize) -> Self {
        let v = |p| fparam(ParamId::Voice(c, p));
        ChannelParams {
            level: fparam(ParamId::Level(c)),
            pan: fparam(ParamId::Pan(c)),
            mute: bparam(ParamId::Mute(c)),
            tone_tune: v(VoiceParam::ToneTune),
            tone_decay: v(VoiceParam::ToneDecay),
            tone_volume: v(VoiceParam::ToneVolume),
            pitch_decay: v(VoiceParam::PitchDecay),
            pitch_decay_vol: v(VoiceParam::PitchDecayVol),
            pm_rate: v(VoiceParam::PhaseModRate),
            pm_level: v(VoiceParam::PhaseModLev),
            noise_volume: v(VoiceParam::NoiseVolume),
            noise_decay: v(VoiceParam::NoiseDecay),
            filter_freq: v(VoiceParam::FilterFreq),
            filter_reso: v(VoiceParam::FilterReso),
            filter_decay: v(VoiceParam::FilterDecay),
            filter_hp: lphp_param(c),
            pan_random: v(VoiceParam::PanRandom),
            nudge: v(VoiceParam::Nudge),
            drift_amount: v(VoiceParam::DriftAmount),
            send_phaser: fparam(ParamId::Send(c, 0)),
            send_dist: fparam(ParamId::Send(c, 1)),
            send_delay: fparam(ParamId::Send(c, 2)),
            send_spring: fparam(ParamId::Send(c, 3)),
        }
    }
}

/// The shared FX section (host automation and project state).
#[derive(Params)]
pub struct FxParams {
    #[id = "ph_rate"]
    pub ph_rate: FloatParam,
    #[id = "ph_depth"]
    pub ph_depth: FloatParam,
    #[id = "ph_color"]
    pub ph_color: FloatParam,
    #[id = "ph_mix"]
    pub ph_mix: FloatParam,
    #[id = "ph_level"]
    pub ph_level: FloatParam,
    #[id = "ph_bypass"]
    pub ph_bypass: BoolParam,
    #[id = "ds_drive"]
    pub ds_drive: FloatParam,
    #[id = "ds_filter"]
    pub ds_filter: FloatParam,
    #[id = "ds_fat"]
    pub ds_fat: FloatParam,
    #[id = "ds_mix"]
    pub ds_mix: FloatParam,
    #[id = "ds_level"]
    pub ds_level: FloatParam,
    #[id = "ds_bypass"]
    pub ds_bypass: BoolParam,
    #[id = "dl_time"]
    pub dl_time: FloatParam,
    #[id = "dl_feedback"]
    pub dl_feedback: FloatParam,
    #[id = "dl_tone"]
    pub dl_tone: FloatParam,
    #[id = "dl_age"]
    pub dl_age: FloatParam,
    #[id = "dl_mod"]
    pub dl_mod: FloatParam,
    #[id = "dl_mix"]
    pub dl_mix: FloatParam,
    #[id = "dl_level"]
    pub dl_level: FloatParam,
    #[id = "dl_bypass"]
    pub dl_bypass: BoolParam,
    #[id = "dl_mode"]
    pub dl_mode: FloatParam,
    #[id = "dl_sync"]
    pub dl_sync: BoolParam,
    #[id = "sp_decay"]
    pub sp_decay: FloatParam,
    #[id = "sp_damping"]
    pub sp_damping: FloatParam,
    #[id = "sp_tension"]
    pub sp_tension: FloatParam,
    #[id = "sp_drive"]
    pub sp_drive: FloatParam,
    #[id = "sp_mix"]
    pub sp_mix: FloatParam,
    #[id = "sp_level"]
    pub sp_level: FloatParam,
    #[id = "sp_bypass"]
    pub sp_bypass: BoolParam,
    #[id = "sp_lowcut"]
    pub sp_low_cut: FloatParam,
    #[id = "sp_highcut"]
    pub sp_high_cut: FloatParam,
}

impl FxParams {
    fn new() -> Self {
        use rp::FxParam::*;
        let f = |p| fparam(ParamId::Fx(p));
        let b = |p| bparam(ParamId::Fx(p));
        FxParams {
            ph_rate: f(PhRate),
            ph_depth: f(PhDepth),
            ph_color: f(PhColor),
            ph_mix: f(PhMix),
            ph_level: f(PhLevel),
            ph_bypass: b(PhBypass),
            ds_drive: f(DsDrive),
            ds_filter: f(DsFilter),
            ds_fat: f(DsFat),
            ds_mix: f(DsMix),
            ds_level: f(DsLevel),
            ds_bypass: b(DsBypass),
            dl_time: f(DlTime),
            dl_feedback: f(DlFeedback),
            dl_tone: f(DlTone),
            dl_age: f(DlAge),
            dl_mod: f(DlMod),
            dl_mix: f(DlMix),
            dl_level: f(DlLevel),
            dl_bypass: b(DlBypass),
            dl_mode: f(DlMode).with_step_size(1.0),
            dl_sync: b(DlSync),
            sp_decay: f(SpDecay),
            sp_damping: f(SpDamping),
            sp_tension: f(SpTension),
            sp_drive: f(SpDrive),
            sp_mix: f(SpMix),
            sp_level: f(SpLevel),
            sp_bypass: b(SpBypass),
            sp_low_cut: f(SpLowCut),
            sp_high_cut: f(SpHighCut),
        }
    }

    fn pref(&self, p: rp::FxParam) -> PRef<'_> {
        use rp::FxParam::*;
        use PRef::*;
        match p {
            PhRate => F(&self.ph_rate),
            PhDepth => F(&self.ph_depth),
            PhColor => F(&self.ph_color),
            PhMix => F(&self.ph_mix),
            PhLevel => F(&self.ph_level),
            PhBypass => B(&self.ph_bypass),
            DsDrive => F(&self.ds_drive),
            DsFilter => F(&self.ds_filter),
            DsFat => F(&self.ds_fat),
            DsMix => F(&self.ds_mix),
            DsLevel => F(&self.ds_level),
            DsBypass => B(&self.ds_bypass),
            DlTime => F(&self.dl_time),
            DlFeedback => F(&self.dl_feedback),
            DlTone => F(&self.dl_tone),
            DlAge => F(&self.dl_age),
            DlMod => F(&self.dl_mod),
            DlMix => F(&self.dl_mix),
            DlLevel => F(&self.dl_level),
            DlBypass => B(&self.dl_bypass),
            DlMode => F(&self.dl_mode),
            DlSync => B(&self.dl_sync),
            SpDecay => F(&self.sp_decay),
            SpDamping => F(&self.sp_damping),
            SpTension => F(&self.sp_tension),
            SpDrive => F(&self.sp_drive),
            SpMix => F(&self.sp_mix),
            SpLevel => F(&self.sp_level),
            SpBypass => B(&self.sp_bypass),
            SpLowCut => F(&self.sp_low_cut),
            SpHighCut => F(&self.sp_high_cut),
        }
    }
}

#[derive(Params)]
pub struct RitmoParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,
    #[persist = "patterns"]
    patterns: PatternStore,
    #[nested(array, group = "Voice")]
    pub ch: [ChannelParams; NUM_CH],
    #[id = "master"]
    pub master: FloatParam,
    #[id = "tempo"]
    pub tempo: FloatParam,
    #[id = "sync"]
    pub sync: BoolParam,
    #[id = "sound"]
    pub sound: BoolParam,
    #[id = "tdrift"]
    pub tempo_drift: FloatParam,
    #[id = "sdrift"]
    pub sound_drift: FloatParam,
    #[id = "swing"]
    pub swing: FloatParam,
    #[id = "accent"]
    pub accent: FloatParam,
    #[nested(group = "FX")]
    pub fx: FxParams,
}

enum PRef<'a> {
    F(&'a FloatParam),
    B(&'a BoolParam),
    /// A switch whose panel value is 0 or 10 (LP/HP): on at FILTER_HP_THRESHOLD and up.
    S(&'a BoolParam),
}

impl RitmoParams {
    fn new(state: Arc<SharedState>) -> Self {
        RitmoParams {
            // opens 1.6x the design size (1344 x 784); SETUP offers 60-160 %
            editor_state: EguiState::from_size((ui::W * ui::START_SCALE) as u32, (ui::H * ui::START_SCALE) as u32),
            patterns: PatternStore(state),
            ch: std::array::from_fn(ChannelParams::new),
            master: fparam(ParamId::Master),
            tempo: fparam(ParamId::Tempo),
            sync: bparam(ParamId::Sync),
            sound: bparam(ParamId::Sound),
            tempo_drift: fparam(ParamId::TempoDrift),
            sound_drift: fparam(ParamId::SoundDrift),
            swing: fparam(ParamId::Swing),
            accent: fparam(ParamId::Accent),
            fx: FxParams::new(),
        }
    }

    fn pref(&self, id: ParamId) -> PRef<'_> {
        use PRef::*;
        match id {
            ParamId::Level(c) => F(&self.ch[c].level),
            ParamId::Pan(c) => F(&self.ch[c].pan),
            ParamId::Mute(c) => B(&self.ch[c].mute),
            ParamId::Master => F(&self.master),
            ParamId::Tempo => F(&self.tempo),
            ParamId::Sync => B(&self.sync),
            ParamId::Sound => B(&self.sound),
            ParamId::TempoDrift => F(&self.tempo_drift),
            ParamId::SoundDrift => F(&self.sound_drift),
            ParamId::Swing => F(&self.swing),
            ParamId::Accent => F(&self.accent),
            ParamId::Send(c, k) => {
                let ch = &self.ch[c];
                F([&ch.send_phaser, &ch.send_dist, &ch.send_delay, &ch.send_spring][k.min(3)])
            }
            ParamId::Fx(p) => self.fx.pref(p),
            ParamId::Voice(c, p) => {
                let ch = &self.ch[c];
                match p {
                    VoiceParam::ToneTune => F(&ch.tone_tune),
                    VoiceParam::ToneDecay => F(&ch.tone_decay),
                    VoiceParam::ToneVolume => F(&ch.tone_volume),
                    VoiceParam::PitchDecay => F(&ch.pitch_decay),
                    VoiceParam::PitchDecayVol => F(&ch.pitch_decay_vol),
                    VoiceParam::PhaseModRate => F(&ch.pm_rate),
                    VoiceParam::PhaseModLev => F(&ch.pm_level),
                    VoiceParam::NoiseVolume => F(&ch.noise_volume),
                    VoiceParam::NoiseDecay => F(&ch.noise_decay),
                    VoiceParam::FilterFreq => F(&ch.filter_freq),
                    VoiceParam::FilterReso => F(&ch.filter_reso),
                    VoiceParam::FilterDecay => F(&ch.filter_decay),
                    VoiceParam::FilterHp => S(&ch.filter_hp),
                    VoiceParam::PanRandom => F(&ch.pan_random),
                    VoiceParam::Nudge => F(&ch.nudge),
                    VoiceParam::DriftAmount => F(&ch.drift_amount),
                }
            }
        }
    }

    fn read(&self, id: ParamId) -> f32 {
        match self.pref(id) {
            PRef::F(f) => f.value(),
            PRef::B(b) => {
                if b.value() {
                    1.0
                } else {
                    0.0
                }
            }
            PRef::S(b) => {
                if b.value() {
                    10.0
                } else {
                    0.0
                }
            }
        }
    }
}

struct Access<'a> {
    p: &'a RitmoParams,
    s: &'a ParamSetter<'a>,
}

impl ParamAccess for Access<'_> {
    fn get(&self, id: ParamId) -> f32 {
        self.p.read(id)
    }
    fn begin(&self, id: ParamId) {
        match self.p.pref(id) {
            PRef::F(f) => self.s.begin_set_parameter(f),
            PRef::B(b) | PRef::S(b) => self.s.begin_set_parameter(b),
        }
    }
    fn set(&self, id: ParamId, plain: f32) {
        match self.p.pref(id) {
            PRef::F(f) => self.s.set_parameter(f, plain),
            PRef::B(b) => self.s.set_parameter(b, plain >= 0.5),
            PRef::S(b) => self.s.set_parameter(b, plain >= rp::FILTER_HP_THRESHOLD),
        }
    }
    fn end(&self, id: ParamId) {
        match self.p.pref(id) {
            PRef::F(f) => self.s.end_set_parameter(f),
            PRef::B(b) | PRef::S(b) => self.s.end_set_parameter(b),
        }
    }
}

/// The plugin's window: its size is the host's to grant (fixed sizes from the SETUP window).
struct PluginWindow<'a> {
    state: &'a EguiState,
}

impl SetupHost for PluginWindow<'_> {
    fn scale(&self, _ctx: &egui::Context) -> f32 {
        let (w, h) = self.state.size();
        (w as f32 / ui::W).min(h as f32 / ui::H)
    }
    fn set_scale(&mut self, _ctx: &egui::Context, scale: f32) {
        self.state.set_requested_size(((ui::W * scale).round() as u32, (ui::H * scale).round() as u32));
    }
}

pub struct Ritmo6 {
    params: Arc<RitmoParams>,
    state: Arc<SharedState>,
    engine: Engine,
}

impl Default for Ritmo6 {
    fn default() -> Self {
        let state = Arc::new(SharedState::default());
        Ritmo6 {
            params: Arc::new(RitmoParams::new(state.clone())),
            state,
            engine: Engine::new(44100.0),
        }
    }
}

const STEREO: Option<std::num::NonZeroU32> = NonZeroU32::new(2);

impl Plugin for Ritmo6 {
    const NAME: &'static str = "Ritmo-6";
    const VENDOR: &'static str = "Geranium";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: STEREO,
            aux_input_ports: &[],
            aux_output_ports: &[new_nonzero_u32(2); NUM_CH],
            names: PortNames {
                layout: Some("Mix + 6 voice outputs"),
                main_input: None,
                main_output: Some("Mix"),
                aux_inputs: &[],
                aux_outputs: &["Voice A", "Voice B", "Voice C", "Voice D", "Voice E", "Voice F"],
            },
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: STEREO,
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn filter_state(state: &mut PluginState) {
        convert_old_lphp(&mut state.params);
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let state = self.state.clone();
        let editor_state = self.params.editor_state.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            ui::UiState::default(),
            |_, _| {},
            move |ctx, setter, st| {
                let access = Access { p: &params, s: setter };
                // No free resize corner: hosts (Cubase above all) handle a continuous drag badly.
                // The SETUP window offers fixed sizes instead, asked of the host in one step; the
                // panel scales to fit and the host keeps the size with the project.
                let mut host = PluginWindow { state: &editor_state };
                // a panic here would cross the host's window code and abort the whole host:
                // catch it, so at worst the panel skips a frame
                let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    ui::draw(ctx, &access, &state, st, &mut host);
                }));
                if drawn.is_err() {
                    nih_log!("Ritmo-6: the panel hit an error and skipped a frame");
                }
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.engine.set_sample_rate(buffer_config.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let params = self.params.clone();
        let vals = Values::read(|id| params.read(id));
        let t = context.transport();
        let transport = Transport { playing: t.playing, pos_beats: t.pos_beats(), tempo: t.tempo };
        self.engine.begin_block(&self.state, &vals, &transport);

        let n = buffer.samples();
        let main = buffer.as_slice();
        let mut next_event = context.next_event();
        for i in 0..n {
            loop {
                match next_event {
                    Some(ev) if ev.timing() as usize <= i => {
                        match ev {
                            NoteEvent::NoteOn { channel, note, velocity, .. } => {
                                self.engine.note_on(&self.state, channel, note, velocity)
                            }
                            NoteEvent::NoteOff { channel, note, .. } => {
                                self.engine.note_off(&self.state, channel, note)
                            }
                            _ => {}
                        }
                        next_event = context.next_event();
                    }
                    _ => break,
                }
            }
            let f = self.engine.next_frame(&self.state, &vals);
            if let Some(l) = main.get_mut(0) {
                l[i] = f.left;
            }
            if let Some(r) = main.get_mut(1) {
                r[i] = f.right;
            }
            for (k, out) in aux.outputs.iter_mut().enumerate().take(NUM_CH) {
                for chan in out.as_slice().iter_mut() {
                    chan[i] = f.voices[k];
                }
            }
        }
        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for Ritmo6 {
    const CLAP_ID: &'static str = "com.geranium.ritmo6";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Six-voice analog-style rhythm machine");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::Instrument, ClapFeature::DrumMachine, ClapFeature::Stereo];
}

impl Vst3Plugin for Ritmo6 {
    const VST3_CLASS_ID: [u8; 16] = *b"Ritmo6RhythmSyn1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Drum];
}

nih_export_clap!(Ritmo6);
nih_export_vst3!(Ritmo6);

#[cfg(test)]
mod tests {
    use super::*;
    use nih_plug::wrapper::state::ParamValue;
    use std::collections::BTreeMap;

    #[test]
    fn old_projects_keep_their_lp_hp_setting() {
        let mut params = BTreeMap::new();
        params.insert("flphp_1".to_string(), ParamValue::F32(0.0));
        params.insert("flphp_2".to_string(), ParamValue::F32(10.0));
        params.insert("flphp_3".to_string(), ParamValue::F32(4.9));
        params.insert("flphp_4".to_string(), ParamValue::F32(5.0));
        params.insert("flphp_5".to_string(), ParamValue::Bool(true)); // already 2.1
        params.insert("tune_1".to_string(), ParamValue::F32(7.5)); // untouched
        convert_old_lphp(&mut params);
        let hp = |k: &str| match params[k] {
            ParamValue::Bool(b) => b,
            _ => panic!("{k} is not a switch"),
        };
        assert!(!hp("flphp_1"));
        assert!(hp("flphp_2"));
        assert!(!hp("flphp_3"));
        assert!(hp("flphp_4"));
        assert!(hp("flphp_5"));
        assert!(matches!(params["tune_1"], ParamValue::F32(v) if v == 7.5));
    }

    #[test]
    fn lp_hp_is_a_switch_for_the_host() {
        let p = RitmoParams::new(Arc::new(SharedState::default()));
        for c in 0..NUM_CH {
            let sw = &p.ch[c].filter_hp;
            assert_eq!(sw.step_count(), Some(1), "voice {c}: the host must see two positions");
            // each voice starts as its factory sound says (some voices are high-pass)
            let id = ParamId::Voice(c, VoiceParam::FilterHp);
            let hp = rp::spec(id).default >= rp::FILTER_HP_THRESHOLD;
            assert_eq!(p.read(id), if hp { 10.0 } else { 0.0 });
            assert_eq!(sw.normalized_value_to_string(1.0, false), "High-pass");
        }
    }
}
