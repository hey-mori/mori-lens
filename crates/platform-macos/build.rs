fn main() {
    #[cfg(target_os = "macos")]
    {
        // Compile the Objective-C bridges.
        cc::Build::new()
            .file("native/overlay.m")
            .flag("-fobjc-arc")
            .compile("lens_overlay");

        cc::Build::new()
            .file("native/vision_ocr.m")
            .flag("-fobjc-arc")
            .compile("lens_vision_ocr");

        println!("cargo:rustc-link-lib=framework=ApplicationServices");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=CoreText");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
        println!("cargo:rustc-link-lib=framework=Vision");
    }
}
