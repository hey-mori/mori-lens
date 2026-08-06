//! Bake the commit this probe was built from into the binary.
//! See `lens_build_stamp` for why, which is longer than this file.

fn main() {
    lens_build_stamp::emit();
}
