//! Ritmo-6 as a VST3 (and CLAP) instrument: stereo mix plus one output per voice.

use nih_plug::params::persist::PersistentField;
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, EguiState};
use ritmo6_core::engine::{Engine, Transport, Values};
use ritmo6_core::params::{self as rp, ParamAccess, ParamId, VoiceParam, NUM_CH, VOICE_LETTERS};
use ritmo6_core::state::{PatternData, SharedState};
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
        ParamId::Level(c) | ParamId::Pan(c) | ParamId::Voice(c, _) => format!("{} {}", VOICE_LETTERS[c], s.name),
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
    #[id = "flphp"]
    pub filter_hp: FloatParam,
    #[id = "panrnd"]
    pub pan_random: FloatParam,
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
            filter_hp: v(VoiceParam::FilterHp),
            pan_random: v(VoiceParam::PanRandom),
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
}

enum PRef<'a> {
    F(&'a FloatParam),
    B(&'a BoolParam),
}

impl RitmoParams {
    fn new(state: Arc<SharedState>) -> Self {
        RitmoParams {
            editor_state: EguiState::from_size(ui::W as u32, ui::H as u32),
            patterns: PatternStore(state),
            ch: std::array::from_fn(ChannelParams::new),
            master: fparam(ParamId::Master),
            tempo: fparam(ParamId::Tempo),
            sync: bparam(ParamId::Sync),
            sound: bparam(ParamId::Sound),
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
                    VoiceParam::FilterHp => F(&ch.filter_hp),
                    VoiceParam::PanRandom => F(&ch.pan_random),
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
            PRef::B(b) => self.s.begin_set_parameter(b),
        }
    }
    fn set(&self, id: ParamId, plain: f32) {
        match self.p.pref(id) {
            PRef::F(f) => self.s.set_parameter(f, plain),
            PRef::B(b) => self.s.set_parameter(b, plain >= 0.5),
        }
    }
    fn end(&self, id: ParamId) {
        match self.p.pref(id) {
            PRef::F(f) => self.s.end_set_parameter(f),
            PRef::B(b) => self.s.end_set_parameter(b),
        }
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

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let state = self.state.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            ui::UiState::default(),
            |_, _| {},
            move |ctx, setter, st| {
                let access = Access { p: &params, s: setter };
                ui::draw(ctx, &access, &state, st);
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
