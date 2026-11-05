; Installer hooks for Mori Lens, read by tauri-bundler (bundle.windows.nsis.installerHooks).

; An upgrade rewrites the shortcuts at the same paths, so Explorer keeps the
; icon it cached for the previous build: an icon change never reached the
; desktop or the Start menu until Explorer restarted. SHCNE_ASSOCCHANGED
; tells the shell to drop its cached icons.
!macro NSIS_HOOK_POSTINSTALL
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
