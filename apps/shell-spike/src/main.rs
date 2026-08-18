//! Does a webview fit beside everything Lens already owns?
//!
//! [#164](https://github.com/swarea/mori-lens/issues/164) stage two. The
//! weighing is done: 22 crates become 383, and it builds here in 1m40s
//! (`docs/benchmarks/2026-08-13-what-a-webview-weighs.md`). What is left is the
//! question that cannot be answered outside the tree.
//!
//! `lens.exe` owns a Win32 message loop, a tray icon, global hotkeys, a layered
//! click-through overlay on its own thread, and a UI Automation apartment.
//! Tauri wants the main thread's event loop and does not give it back. **That
//! is a structural conflict rather than an incidental one**, and it is what
//! this measures.
//!
//! # This is a spike and not a product
//!
//! It is excluded from the workspace on purpose, so `scripts/check.sh` never
//! builds it. Nothing here is wired into `apps/lens`, and deleting this
//! directory leaves the product exactly as it is.
//!
//! It reuses the **real** components rather than imitations: the overlay, the
//! tray and the acquirer are the ones the product ships. An imitation that
//! coexists says nothing about the thing that has to.

use std::sync::mpsc;

/// What survived, in the order it was tried.
#[derive(Debug, Default)]
struct Survived {
    overlay: bool,
    tray: bool,
    acquire: bool,
    card_drawn: bool,
    hotkey: bool,
}

fn main() {
    println!("shell spike · does a webview fit beside the message loop\n");

    // Everything is started from Tauri's `setup`, which runs on the main thread
    // before the event loop takes it. That is the honest order: if a component
    // has to be first, this is where it finds out.
    let (report, wait) = mpsc::channel::<Survived>();

    let built = tauri::Builder::default()
        .setup(move |_app| {
            let mut survived = Survived::default();

            // The overlay owns a thread and a message pump of its own.
            let overlay = lens_platform_windows::Overlay::start();
            survived.overlay = overlay.is_some();
            println!(
                "  overlay      {}",
                if survived.overlay { "started" } else { "FAILED" }
            );

            // The tray is a window plus a shell icon on the calling thread.
            let tray = lens_platform_windows::Tray::start(
                lens_core::theme::Rgb::new(0x7d, 0x4f, 0x14),
                "shell spike",
            );
            survived.tray = tray.is_some();
            println!(
                "  tray         {}",
                if survived.tray { "created" } else { "FAILED" }
            );

            // A COM apartment, entered by a call site that tolerates
            // RPC_E_CHANGED_MODE. Tauri has already initialised one by now.
            match lens_platform_windows::UiaTextAcquirer::new() {
                Ok(_) => {
                    survived.acquire = true;
                    println!("  acquirer     built, so the apartment was shared");
                }
                Err(error) => println!("  acquirer     FAILED: {error:?}"),
            }

            // And the thing a reader would actually notice: a card on screen
            // while a webview is up.
            if let Some(overlay) = &overlay {
                let card = lens_core::card::Card::not_readable("shell spike");
                overlay.show(
                    card,
                    lens_core::geometry::ScreenRect::new(80, 80, 1, 1),
                );
                survived.card_drawn = true;
                println!("  card         asked for at 80,80");
            }

            // A global hotkey posts WM_HOTKEY to the thread that registered
            // it, and that thread's loop has to dispatch it. Registration is
            // what a machine can check here; **delivery needs a keypress and
            // is therefore untested**, which is the honest limit of this run.
            // SAFETY: a documented registration on the calling thread, matched
            // by no unregistration because this process is about to be killed.
            survived.hotkey = unsafe {
                windows::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey(
                    None,
                    0xB0B,
                    windows::Win32::UI::Input::KeyboardAndMouse::MOD_CONTROL
                        | windows::Win32::UI::Input::KeyboardAndMouse::MOD_ALT,
                    b'J' as u32,
                )
                .is_ok()
            };
            println!(
                "  hotkey       {}",
                if survived.hotkey {
                    "registered, delivery untested"
                } else {
                    "FAILED to register"
                }
            );

            let _ = report.send(survived);
            // Held for the life of the process, so nothing is destroyed before
            // the window is even up.
            std::mem::forget(overlay);
            std::mem::forget(tray);
            Ok(())
        })
        .build(tauri::generate_context!());

    match built {
        Ok(app) => {
            if let Ok(survived) = wait.recv_timeout(std::time::Duration::from_secs(30)) {
                println!("\n  {survived:?}");
            }
            println!("\n  the window is up. Close it to end the spike.");
            app.run(|_, _| {});
        }
        Err(error) => println!("\n  tauri did not build: {error}"),
    }
}
