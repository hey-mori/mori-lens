//! Bake the commit this build came from into the binary.
//!
//! The probe has had this since its first sweep. The product had not, and on
//! 2026-08-06 a reader pasted a session log that neither of us could date: a
//! defect fixed an hour earlier was read as still present because there was no
//! way to tell which binary had produced the lines.
//!
//! See `lens_build_stamp`.

fn main() {
    lens_build_stamp::emit();
}
