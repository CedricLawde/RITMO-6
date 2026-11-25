# vendor/

`nih_plug_egui/` is a copy of the `nih_plug_egui` crate of [nih-plug](https://github.com/robbert-vdh/nih-plug)
at revision `de421011` (ISC licence, see `nih_plug_egui/LICENSE`), used in place of the git version through
`[patch]` in the root `Cargo.toml`.

One change: `EguiState::set_requested_size` is public, so the plugin can ask the host for one of its fixed
window sizes (60 / 80 / 100 / 130 / 160 %) in one step from the SETUP menu, instead of a continuous drag on a
resize corner (which Cubase and some CLAP hosts handle badly). Its own `Cargo.toml` points `nih_plug` at the
same git revision instead of a path inside the nih-plug repository.
