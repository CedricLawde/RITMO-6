//! Ritmo-6 core: sound engine, sequencer, pattern memory and front panel.
//! Shared by the VST3 plugin (plugin/) and the standalone app (src/main.rs).

pub mod bank;
pub mod classic;
pub mod dsp;
pub mod engine;
pub mod fx;
pub mod kits;
pub mod params;
pub mod presets;
pub mod setup;
pub mod state;
pub mod update;
pub mod ui;

pub use egui;
