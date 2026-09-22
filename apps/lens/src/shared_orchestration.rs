// Shared orchestration logic between Windows and macOS platform adapters.
//
// This module extracts business logic that is identical across platforms,
// reducing duplication and ensuring consistency. Platform-specific code
// (UIA vs AX acquisition, overlay/tray/menubar, hover worker architecture)
// remains in the respective platform files.

use crate::library_service::SharedLibrary;
use crate::settings_service::SharedSettings;
use crate::tauri_app::SharedRecent;
use lens_core::card::{Card, CardBody};

/// Record a presented reading to both session-recent and persistent history.
///
/// Called after a card is shown to the reader. Session-recent is always updated
/// (memory-only, cleared on exit). Persistent history is only written when the
/// reader has turned it on in settings.
///
/// This is shared between Windows and macOS because the recording logic is
/// pure business logic with no platform dependency.
pub fn record_presented_reading(
    card: &Card,
    source_app: Option<&str>,
    target_language: &str,
    recent: &SharedRecent,
    library: &SharedLibrary,
    settings: &SharedSettings,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let app_name = source_app.map(str::to_owned);
    let answer_text = match &card.body {
        CardBody::Translation(t) => t.clone(),
        CardBody::Analysis { translation, .. } => translation.clone().unwrap_or_default(),
        _ => String::new(),
    };
    if !answer_text.is_empty() {
        let recent_card = lens_core::recent::RecentCard {
            original: card.original.clone(),
            answer: answer_text.clone(),
            shown_at: now,
            app: app_name.clone(),
        };
        recent.lock().unwrap().record(recent_card);
    }

    // Record to persistent history (ADR-0014 Tier 3).
    // Only when the reader has turned it on; off means nothing is written.
    {
        let current = settings.lock().unwrap().current().clone();
        if current.history {
            let excluded = app_name
                .as_ref()
                .map(|name| {
                    current
                        .history_excluded
                        .iter()
                        .any(|ex| name.to_lowercase().contains(&ex.to_lowercase()))
                })
                .unwrap_or(false);
            if !excluded && !answer_text.is_empty() {
                let kind = match &card.body {
                    CardBody::Analysis { .. } => "analysis",
                    _ => "translation",
                };
                let meta = card.metadata();
                let provenance = match meta.map(|m| m.provenance) {
                    Some(lens_core::card::CardProvenance::Ocr) => "ocr",
                    _ => "accessibility",
                };
                let provider_scope = match meta.and_then(|m| m.provider_scope) {
                    Some(lens_core::card::CardProviderScope::External) => "external",
                    Some(lens_core::card::CardProviderScope::Local) => "local",
                    None => "unknown",
                };
                let model = meta.and_then(|m| m.model.clone());
                let provider = meta.and_then(|m| m.provider.map(|p| p.as_str().to_owned()));
                let stored_app = if current.history_store_app {
                    app_name
                } else {
                    None
                };
                let entry = lens_storage::HistoryEntry {
                    id: 0,
                    original: card.original.clone(),
                    answer: answer_text,
                    kind: kind.to_owned(),
                    source_language: None,
                    target_language: Some(target_language.to_owned()),
                    model,
                    provider,
                    app: stored_app,
                    provenance: provenance.to_owned(),
                    provider_scope: provider_scope.to_owned(),
                    created_at: now,
                };
                let lib = library.lock().unwrap();
                if let Err(e) = lib.library().record_history(&entry) {
                    eprintln!("  history   record failed: {e}");
                }
            }
        }
    }
}
