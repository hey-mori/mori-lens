//! The session-recent tier of history ([ADR-0014](../../../docs/decisions/ADR-0014-three-tiers-of-history.md)).
//!
//! Memory only, gone when the process exits. The latest 50 cards or two hours,
//! whichever prunes first. Consecutive answers for the same target are
//! deduplicated.

use std::collections::VecDeque;

/// A card the reader saw during this session.
#[derive(Debug, Clone)]
pub struct RecentCard {
    /// The original text on the screen.
    pub original: String,
    /// The answer (translation or explanation).
    pub answer: String,
    /// When the card was shown, seconds since the Unix epoch.
    pub shown_at: i64,
    /// The source application's process name, if known.
    pub app: Option<String>,
}

/// The session-recent list.
///
/// Holds up to 50 cards or 2 hours of history. Consecutive duplicates are
/// skipped.
#[derive(Debug)]
pub struct SessionRecent {
    cards: VecDeque<RecentCard>,
    /// The maximum number of cards to keep.
    max_cards: usize,
    /// The maximum age in seconds (2 hours = 7200).
    max_age_secs: i64,
}

impl SessionRecent {
    /// Create a new session-recent list with default limits.
    pub fn new() -> Self {
        Self {
            cards: VecDeque::new(),
            max_cards: 50,
            max_age_secs: 7200,
        }
    }

    /// Record a card the reader just saw.
    ///
    /// Consecutive duplicates (same original text) are skipped. If the list
    /// exceeds the maximum size, the oldest card is removed.
    pub fn record(&mut self, card: RecentCard) {
        // Skip consecutive duplicates.
        if let Some(last) = self.cards.back() {
            if last.original == card.original {
                return;
            }
        }

        self.cards.push_back(card);

        // Prune by size.
        while self.cards.len() > self.max_cards {
            self.cards.pop_front();
        }
    }

    /// Get the current list of recent cards, newest first.
    ///
    /// Cards older than the maximum age are pruned before returning.
    pub fn list(&mut self, now: i64) -> Vec<RecentCard> {
        let cutoff = now - self.max_age_secs;

        // Prune by age.
        while let Some(front) = self.cards.front() {
            if front.shown_at < cutoff {
                self.cards.pop_front();
            } else {
                break;
            }
        }

        // Return newest first.
        self.cards.iter().rev().cloned().collect()
    }

    /// The number of cards currently held.
    pub fn len(&self) -> usize {
        self.cards.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

impl Default for SessionRecent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_card(original: &str, at: i64) -> RecentCard {
        RecentCard {
            original: original.to_owned(),
            answer: "answer".to_owned(),
            shown_at: at,
            app: None,
        }
    }

    #[test]
    fn records_cards_in_order() {
        let mut recent = SessionRecent::new();
        recent.record(a_card("first", 1000));
        recent.record(a_card("second", 1001));

        let list = recent.list(1002);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].original, "second");
        assert_eq!(list[1].original, "first");
    }

    #[test]
    fn skips_consecutive_duplicates() {
        let mut recent = SessionRecent::new();
        recent.record(a_card("same", 1000));
        recent.record(a_card("same", 1001));
        recent.record(a_card("same", 1002));

        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn does_not_skip_non_consecutive_duplicates() {
        let mut recent = SessionRecent::new();
        recent.record(a_card("first", 1000));
        recent.record(a_card("second", 1001));
        recent.record(a_card("first", 1002));

        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn prunes_by_size() {
        let mut recent = SessionRecent {
            max_cards: 3,
            ..SessionRecent::new()
        };

        for i in 0..5 {
            recent.record(a_card(&format!("card {i}"), 1000 + i));
        }

        assert_eq!(recent.len(), 3);
        let list = recent.list(1010);
        assert_eq!(list[0].original, "card 4");
        assert_eq!(list[2].original, "card 2");
    }

    #[test]
    fn prunes_by_age() {
        let mut recent = SessionRecent {
            max_age_secs: 100,
            ..SessionRecent::new()
        };

        recent.record(a_card("old", 1000));
        recent.record(a_card("new", 1150));

        let list = recent.list(1150);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].original, "new");
    }

    /// SessionRecent is memory-only: dropping and recreating loses all data.
    /// This is the M8 contract — SessionRecent never persists to disk.
    #[test]
    fn session_recent_does_not_survive_drop() {
        let mut recent = SessionRecent::new();
        recent.record(a_card("persisted?", 1000));
        assert_eq!(recent.list(1000).len(), 1);

        drop(recent);

        let mut fresh = SessionRecent::new();
        assert!(
            fresh.list(1000).is_empty(),
            "SessionRecent must not persist across instances"
        );
    }
}
