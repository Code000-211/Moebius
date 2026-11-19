//! How far off the estimates of cards were: the Retro compares the points of the cards that reached Done that day with
//! the tokens and the time they took. Points are worth what they were worth on the cards that are done: the typical
//! tokens and minutes of one point (the median over every done card that has an estimate and cost something), so a
//! card is far off when it took many times, or a small part of, what its points would have. It says nothing of a
//! card that cost nothing, and nothing at all while no done card can say what a point is worth.

use serde::{Deserialize, Serialize};

use crate::app::Result;
use crate::store::Store;

/// The most cards a report lists as far off.
const LISTED: usize = 5;

/// A card whose estimate was far off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstimateMiss {
    pub card: i64,
    pub title: String,
    /// The card's estimate in points.
    pub estimate: u32,
    /// The tokens and minutes its runs took.
    pub tokens: i64,
    pub minutes: i64,
    /// How many times the tokens the estimate called for the card took: 3 is three times as many, 0.25 a quarter.
    pub tokens_ratio: f64,
    /// The same for the time.
    pub time_ratio: f64,
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied()
}

/// How far a ratio is from 1, in either direction: 4 and 0.25 are equally far.
fn distance(ratio: f64) -> f64 {
    ratio.ln().abs()
}

/// A done card with an estimate and what it cost.
struct Done {
    id: i64,
    title: String,
    points: u32,
    tokens: f64,
    minutes: f64,
}

fn done_cards(store: &Store, window: Option<(i64, i64)>) -> Result<Vec<Done>> {
    let mut out = Vec::new();
    for (id, tokens, ms) in store.done_card_costs(window)? {
        let Some(card) = store.card(id)? else {
            continue;
        };
        if let Some(points) = card.estimate {
            out.push(Done {
                id,
                title: card.title,
                points,
                tokens: tokens as f64,
                minutes: ms as f64 / 60_000.0,
            });
        }
    }
    Ok(out)
}

/// The median cost of one point over the cards that cost something.
fn per_point(all: &[Done], cost: fn(&Done) -> f64) -> Option<f64> {
    median(
        all.iter()
            .filter(|c| cost(c) > 0.0)
            .map(|c| cost(c) / f64::from(c.points))
            .collect(),
    )
}

/// The cards that reached Done in `window` (`[from, to)`) whose estimate was furthest off, the furthest first, at most
/// five. A card is listed only when it took at least half as much again, or at most two thirds, of what its
/// points are worth in tokens, or at least half as much again in time.
pub fn furthest_off(store: &Store, window: (i64, i64)) -> Result<Vec<EstimateMiss>> {
    let all = done_cards(store, None)?;
    let Some(tokens_per_point) = per_point(&all, |c| c.tokens) else {
        return Ok(Vec::new());
    };
    let minutes_per_point = per_point(&all, |c| c.minutes);
    let mut misses: Vec<EstimateMiss> = done_cards(store, Some(window))?
        .into_iter()
        .filter(|c| c.tokens > 0.0)
        .map(|c| {
            let ratio = |actual: f64, per_point: Option<f64>| {
                per_point.map_or(1.0, |p| actual / (p * f64::from(c.points)))
            };
            EstimateMiss {
                card: c.id,
                tokens_ratio: ratio(c.tokens, Some(tokens_per_point)),
                time_ratio: ratio(c.minutes, minutes_per_point),
                title: c.title,
                estimate: c.points,
                tokens: c.tokens as i64,
                minutes: c.minutes.round() as i64,
            }
        })
        .filter(|m| m.tokens_ratio >= 1.5 || m.tokens_ratio <= 2.0 / 3.0 || m.time_ratio >= 1.5)
        .collect();
    // The furthest first, by whichever of tokens and time was further off.
    let off = |m: &EstimateMiss| distance(m.tokens_ratio).max(distance(m.time_ratio));
    misses.sort_by(|a, b| off(b).total_cmp(&off(a)).then(a.card.cmp(&b.card)));
    for m in &mut misses {
        // Rounded only to be shown, after the threshold was applied to the exact ratios.
        m.tokens_ratio = (m.tokens_ratio * 100.0).round() / 100.0;
        m.time_ratio = (m.time_ratio * 100.0).round() / 100.0;
    }
    misses.truncate(LISTED);
    Ok(misses)
}
