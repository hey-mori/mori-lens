//! Bake the commit this build came from into the binary, and run tauri-build.
//!
//! The probe has had the build stamp since its first sweep. The product had
//! not, and on 2026-08-06 a reader pasted a session log that neither of us
//! could date: a defect fixed an hour earlier was read as still present
//! because there was no way to tell which binary had produced the lines.
//!
//! `tauri_build` generates the WebView2 bootstrapper and bundles the frontend
//! assets declared in `tauri.conf.json`.

fn main() {
    lens_build_stamp::emit();
    tauri_build::build();
}
