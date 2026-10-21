//! Groups scanned items into proposed events by time gap and calendar day. Pure.

use std::collections::HashMap;

use chrono::{Duration, NaiveDateTime};
use serde::{Deserialize, Serialize};

use super::scan::{ItemKind, ScanItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Named,
    Everyday,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposedEvent {
    /// `ScanItem::idx` values, in offset-corrected capture order.
    pub item_idxs: Vec<usize>,
    /// Offset-corrected times of the first and last item.
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub kind: EventKind,
    /// Empty for now; naming suggestions arrive with the import UI.
    pub suggested_name: String,
}

/// Splits `items` into events. Times are camera-local and naive, so the "day"
/// boundary is simply the calendar date of the offset-corrected time. A gap
/// of exactly `gap_minutes` stays in the same event. Events with fewer than
/// `everyday_max` photos (videos do not count) are `Everyday`.
pub fn split_into_events(
    items: &[ScanItem],
    offsets: &HashMap<String, i64>,
    gap_minutes: u32,
    everyday_max: usize,
) -> Vec<ProposedEvent> {
    let mut order: Vec<(NaiveDateTime, &ScanItem)> = items
        .iter()
        .map(|it| {
            let secs = offsets.get(&it.camera).copied().unwrap_or(0);
            (it.capture_time + Duration::seconds(secs), it)
        })
        .collect();
    // idx tiebreak keeps burst frames with identical timestamps in shooting order.
    order.sort_by_key(|(t, it)| (*t, it.idx));

    let gap = Duration::minutes(i64::from(gap_minutes));
    let mut events: Vec<ProposedEvent> = Vec::new();
    let mut photos_in_current = 0usize;

    for (time, item) in order {
        let continues = events
            .last()
            .is_some_and(|e| time - e.end <= gap && time.date() == e.end.date());
        if !continues {
            if let Some(done) = events.last_mut() {
                done.kind = kind_for(photos_in_current, everyday_max);
            }
            photos_in_current = 0;
            events.push(ProposedEvent {
                item_idxs: Vec::new(),
                start: time,
                end: time,
                kind: EventKind::Named,
                suggested_name: String::new(),
            });
        }
        let current = events.last_mut().expect("event pushed above");
        current.item_idxs.push(item.idx);
        current.end = time;
        if item.kind == ItemKind::Photo {
            photos_in_current += 1;
        }
    }
    if let Some(done) = events.last_mut() {
        done.kind = kind_for(photos_in_current, everyday_max);
    }
    events
}

fn kind_for(photos: usize, everyday_max: usize) -> EventKind {
    if photos < everyday_max {
        EventKind::Everyday
    } else {
        EventKind::Named
    }
}
