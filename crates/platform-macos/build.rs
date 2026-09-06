fn main() {
    #[cfg(target_os = "macos")]
    {
        // Explicitly track all native sources and headers for rebuild invalidation.
        // Without this, Cargo may use cached artifacts when .m/.h files change.
        println!("cargo:rerun-if-changed=native/overlay.m");
        println!("cargo:rerun-if-changed=native/overlay.h");
        println!("cargo:rerun-if-changed=native/vision_ocr.m");
        println!("cargo:rerun-if-changed=native/vision_ocr.h");
        println!("cargo:rerun-if-changed=native/capture.m");
        println!("cargo:rerun-if-changed=native/capture.h");
        println!("cargo:rerun-if-changed=native/menubar.m");
        println!("cargo:rerun-if-changed=native/menubar.h");
        println!("cargo:rerun-if-changed=native/clipboard.m");
        println!("cargo:rerun-if-changed=native/clipboard.h");
        println!("cargo:rerun-if-changed=native/lifecycle.m");
        println!("cargo:rerun-if-changed=native/lifecycle.h");
        println!("cargo:rerun-if-changed=native/hotkey.m");
        println!("cargo:rerun-if-changed=native/hotkey.h");

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
