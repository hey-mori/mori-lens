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

        cc::Build::new()
            .file("native/capture.m")
            .flag("-fobjc-arc")
            .compile("lens_capture");

        cc::Build::new()
            .file("native/menubar.m")
            .flag("-fobjc-arc")
            .compile("lens_menubar");

        cc::Build::new()
            .file("native/clipboard.m")
            .flag("-fobjc-arc")
            .compile("lens_clipboard");

        cc::Build::new()
            .file("native/lifecycle.m")
            .flag("-fobjc-arc")
            .compile("lens_lifecycle");

        cc::Build::new()
            .file("native/hotkey.m")
            .flag("-fobjc-arc")
            .compile("lens_hotkey");

        println!("cargo:rustc-link-lib=framework=ApplicationServices");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=CoreText");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
        println!("cargo:rustc-link-lib=framework=Vision");
        println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
        println!("cargo:rustc-link-lib=framework=CoreVideo");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=Carbon");
    }
}
