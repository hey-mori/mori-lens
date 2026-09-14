# P3 Tauri Capability Audit

**Date:** 2026-09-15  
**Auditor:** External security review  
**Tauri Version:** 2.x (from `apps/lens/Cargo.toml` line 20)

## Executive Summary

**Finding: Current configuration grants both windows identical permissions, but this is acceptable for the current command set.**

The detail window (passive/pinned explanation surface) technically has access to all 44 registered Tauri commands, but:
1. Tauri v2 capabilities restrict **core APIs** (filesystem, shell, dialog, etc.), not custom commands
2. Custom commands (`#[tauri::command]`) are application-controlled and can implement their own authorization logic
3. The detail window's frontend code only calls 6 commands, all `detail_*` prefixed
4. No over-permission exists at the capability layer because custom commands are not capability-gated in Tauri v2

## Inventory

### Registered Tauri Commands (44 total)

**System/Lifecycle (5):**
- `get_diagnostics` - Read diagnostics metadata
- `get_runtime_status` - Check Ollama/AI status
- `set_autostart` - Toggle autostart registry entry
- `request_pause_toggle` - Pause/resume hover detection
- `request_quit` - Exit application

**Settings (2):**
- `get_settings` - Read configuration
- `save_settings` - Write configuration

**AI/Models (4):**
- `list_models` - Query Ollama endpoint
- `check_ollama` - Test Ollama connectivity
- `get_local_ai_status` - Check local AI availability
- `test_model` - Test translation with model

**Library CRUD (13):**
- `library_query` - Search/list items
- `library_get` - Get single item
- `library_delete` - Delete item
- `library_update_note` - Edit note
- `library_set_tags` - Set tags
- `library_get_tags` - Get tags
- `library_distinct_apps` - Get app list
- `library_distinct_source_languages` - Get source languages
- `library_distinct_target_languages` - Get target languages
- `library_distinct_tags` - Get tag list
- `library_export` - Export as Markdown
- `library_export_json` - Export as JSON
- `library_export_jsonl` - Export as JSONL
- `library_import_jsonl` - Import from JSONL
- `library_secure_wipe` - **Destructive**: Delete all data

**Review (3):**
- `library_later_count` - Count later items
- `library_review_session` - Start review session
- `library_set_review_state` - Mark known/archive/later

**History (5):**
- `history_list` - List history entries
- `history_count` - Count history entries
- `history_delete` - Delete history entry
- `history_promote` - Promote to library
- `history_prune` - Delete old entries

**Mori Transfer (2):**
- `mori_status` - Check Mori availability
- `mori_send_item` - Send item to Mori

**Recent (1):**
- `recent_list` - List session-only recent items

**Detail Window (6):**
- `detail_read` - Read pinned explanation
- `detail_copy` - Copy to clipboard
- `detail_save` - Save to library
- `detail_save_note` - Update note
- `detail_request_explain` - Request re-explanation
- `detail_open_in_library` - Navigate main window to item

**Diagnostics (1):**
- `export_diagnostics` - Export metadata JSON

### Frontend Command Usage

**Main Window (index.html → js/app.js):**
Calls all commands except `detail_*` family. Imports:
- `library.js` - Library CRUD, filters, export/import
- `review.js` - Review session management
- `history.js` - History list/delete/prune
- `models.js` - AI model configuration
- `diagnostics.js` - Diagnostics export, secure wipe
- `status.js` - Runtime status, settings

**Detail Window (detail.html inline JS):**
Calls only 6 commands:
```javascript
invoke('detail_read')
invoke('detail_copy')
invoke('detail_save')
invoke('detail_request_explain')
invoke('detail_open_in_library')
invoke('detail_save_note', { id, note })
```

### Current Capability Configuration

**File:** `apps/lens/capabilities/default.json`
```json
{
  "identifier": "default",
  "description": "Default capabilities for the Mori Lens windows",
  "windows": ["main", "detail"],
  "permissions": [
    "core:default",
    "core:event:default"
  ]
}
```

**Analysis:**
- Both windows receive identical permissions
- `core:default` grants basic window management
- `core:event:default` grants event listen/emit
- No filesystem, shell, dialog, clipboard, or other plugin permissions
- Custom commands are **not** gated by capabilities in Tauri v2

## Tauri v2 Capability Model

### What Capabilities Control

Tauri v2 capabilities restrict **plugin APIs** and **core features**:
- Filesystem access (`fs:allow-*`, `fs:deny-*`)
- Shell commands (`shell:allow-*`, `shell:deny-*`)
- Dialog APIs (`dialog:allow-*`, `dialog:deny-*`)
- Clipboard (`clipboard:allow-*`, `clipboard:deny-*`)
- Window management (`window:allow-*`, `window:deny-*`)
- HTTP requests (`http:allow-*`, `http:deny-*`)
- Event system (`event:allow-*`, `event:deny-*`)

### What Capabilities Do NOT Control

**Custom `#[tauri::command]` functions are not capability-gated.**

Custom commands are registered in `invoke_handler` and are callable from any window that has access to `window.__TAURI__.core.invoke`. Authorization must be implemented **inside the command itself** or via frontend discipline.

## Risk Assessment

### Detail Window Over-Permission

**Theoretical Risk:** Detail window's JavaScript could call any of the 44 commands:
```javascript
// These would technically work if called from detail.html:
invoke('library_secure_wipe')  // Destructive
invoke('set_autostart', { enabled: true })
invoke('request_quit')
invoke('save_settings', { ... })
invoke('mori_send_item', { item_id: 42 })
```

**Actual Risk:** **Low**
1. Detail window frontend code only calls `detail_*` commands
2. No UI elements trigger destructive operations
3. Detail window is a passive display surface, not a settings interface
4. A malicious actor would need to inject JavaScript into the detail window, which requires compromising the Tauri webview

### Mitigation Options

#### Option A: Frontend Discipline (Current State)
**Status:** ✅ Acceptable for now

Rely on frontend code to only call appropriate commands. This is the standard pattern for Tauri applications.

**Pros:**
- Simple configuration
- No performance overhead
- Standard Tauri pattern

**Cons:**
- No hard enforcement
- Accidental misuse possible
- Malicious JS injection could exploit

#### Option B: Per-Window Command Guards (Not Implemented)
**Status:** ⚠️ Not supported by Tauri v2 capabilities

Tauri v2 does not provide a built-in mechanism to restrict custom commands per window via capabilities. Implementation would require:

1. **Command-level authorization:**
```rust
#[tauri::command]
async fn library_secure_wipe(window: Window, ...) -> Result<(), String> {
    // Check which window is calling
    if window.label() != "main" {
        return Err("Command not allowed from this window".into());
    }
    // ... proceed
}
```

2. **Helper macro for DRY:**
```rust
macro_rules! require_main_window {
    ($window:expr) => {
        if $window.label() != "main" {
            return Err("Command requires main window".into());
        }
    };
}
```

**Pros:**
- Hard enforcement
- Prevents accidental misuse
- Defense in depth

**Cons:**
- Boilerplate on every command
- Runtime overhead (minimal)
- Not standard Tauri pattern

#### Option C: Separate Capability Files (Partial Solution)
**Status:** ⚠️ Does not solve custom command issue

Create separate capabilities for main and detail windows:

**File:** `apps/lens/capabilities/main.json`
```json
{
  "identifier": "main",
  "windows": ["main"],
  "permissions": ["core:default", "core:event:default"]
}
```

**File:** `apps/lens/capabilities/detail.json`
```json
{
  "identifier": "detail",
  "windows": ["detail"],
  "permissions": ["core:default", "core:event:default"]
}
```

**Limitation:** This only restricts **core/plugin APIs**, not custom commands. Since both windows currently use identical permissions, this provides no additional security for custom commands.

## Recommendation

### Short-Term (Current State)

**Status: ACCEPTABLE**

The current configuration is acceptable because:
1. Detail window frontend only calls appropriate commands
2. No destructive operations are exposed in detail window UI
3. Tauri webview is sandboxed; JS injection requires a separate vulnerability
4. This is the standard Tauri pattern for multi-window applications

**Action:** Document the design decision and monitor for changes.

### Medium-Term (If Security Requirements Increase)

**Recommendation:** Implement command-level guards for destructive operations.

**Priority commands to guard:**
1. `library_secure_wipe` - Data destruction
2. `request_quit` - Application lifecycle
3. `set_autostart` - System configuration
4. `save_settings` - Configuration mutation
5. `mori_send_item` - External data transfer

**Implementation:**
```rust
#[tauri::command]
async fn library_secure_wipe(
    window: tauri::Window,
    library: State<'_, SharedLibrary>,
) -> Result<(), String> {
    // Guard: only main window can wipe library
    if window.label() != "main" {
        return Err("library_secure_wipe requires main window".into());
    }
    
    // ... proceed with wipe
}
```

**Scope:** Apply guards to ~5 high-risk commands, not all 44.

### Long-Term (If Tauri v2 Evolves)

**Monitor:** Tauri v2 may add capability-based command restrictions in future versions.

**Watch:**
- Tauri RFCs for capability improvements
- Community plugins for command authorization
- Migration to Tauri v3 if it offers better security primitives

## Evidence

### Verification Steps

1. ✅ Identified Tauri version from `apps/lens/Cargo.toml` line 20
2. ✅ Read `apps/lens/capabilities/default.json` showing both windows get identical permissions
3. ✅ Inventoried all 44 registered commands in `apps/lens/src/tauri_app.rs` lines 1008-1051
4. ✅ Verified detail window frontend only calls 6 `detail_*` commands in `apps/lens/web/detail.html`
5. ✅ Verified main window frontend calls all other commands via `js/app.js` and imported modules
6. ✅ Confirmed Tauri v2 capabilities do not gate custom commands (only plugin/core APIs)

### Command Inventory Commands

**High-Risk Commands (candidates for guards):**
- `library_secure_wipe` - Destructive
- `request_quit` - Lifecycle
- `set_autostart` - System config
- `save_settings` - Mutation
- `mori_send_item` - External transfer

**Low-Risk Commands (no guards needed):**
- All `detail_*` commands (already detail-only by design)
- All read-only commands (`library_query`, `library_get`, etc.)
- All metadata commands (`get_diagnostics`, `get_runtime_status`, etc.)

## Conclusion

**Finding: No over-permission exists at the capability layer.**

The current configuration is acceptable for a desktop application with a trusted frontend. The detail window technically has access to all commands, but frontend discipline prevents misuse. If security requirements increase, implement command-level guards for the 5 high-risk operations identified above.

**Status:** No action required at this time. Document and monitor.
