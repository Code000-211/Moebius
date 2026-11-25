//! Analytics (issue #389): what the moves and runs the store keeps say about the work, across the organisation and per
//! project: how many cards and points are done each day, how long a card takes from its start to done and from its
//! making to done, how long cards sit in each column, the burn-up of each cycle and each milestone, and the tokens
//! per done point.
//!
//! Everything is read from the cards the asker may see (`App::sight`), so a private project's cards are in no figure,
//! no total and no cycle or milestone; archived cards stay in. The window of days sets which done cards and which
//! moves count; the repository and agent filters narrow the cards, and so every figure, as they do the usage page.
//! The facts of the cards are read by `store::analytics`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use time::{Date, Duration, format_description};

use super::cycles::{Planned, burn_up_between, day_of};
use super::{App, AppError, BurnUpDay, Result};
use crate::decide::Actor;
use crate::store::{AnalyticsCard, CardFilter};

/// The most days a window reaches back.
pub const DAYS_LIMIT: i64 = 365;
/// The window when none is asked for.
pub const DEFAULT_DAYS: i64 = 30;
/// [`DEFAULT_DAYS`] under the name the crate exports.
pub use DEFAULT_DAYS as ANALYTICS_DEFAULT_DAYS;
/// The most cycles and the most milestones the answer lists, the latest first.
pub const BURN_UP_LIMIT: usize = 12;
/// The class of the columns cards finish in; the time spent in them is not counted.
const DONE: &str = crate::app::columns::DONE;

/// What narrows the analytics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsQuery {
    /// The window, this many local days with today.
    pub days: i64,
    /// The cards of one repository.
    pub repository: Option<i64>,
    /// The cards one agent ran something on; their tokens are that agent's.
    pub agent: Option<String>,
}

impl Default for AnalyticsQuery {
    fn default() -> Self {
        Self {
            days: DEFAULT_DAYS,
            repository: None,
            agent: None,
        }
    }
}

/// The analytics of a window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Analytics {
    /// Today's date on the server, `YYYY-MM-DD`.
    pub today: String,
    pub days: i64,
    pub totals: AnalyticsTotals,
    /// Every day of the window, oldest first, with what was done on it.
    pub throughput: Vec<ThroughputDay>,
    /// Start to done, of the cards done in the window that were started.
    pub cycle_time: Option<Spread>,
    /// Making to done, of the cards done in the window.
    pub lead_time: Option<Spread>,
    /// The time cards spent in each column, the column with the longest mean first.
    pub columns: Vec<ColumnTime>,
    /// The projects with cards in the figures.
    pub projects: Vec<ProjectFigures>,
    /// The latest cycles with their burn-up.
    pub cycles: Vec<BurnUp>,
    /// The milestones with the latest target dates, with their burn-up.
    pub milestones: Vec<BurnUp>,
}

/// The totals of a window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyticsTotals {
    /// The cards done in the window.
    pub done: i64,
    /// Their points, those with an estimate.
    pub points_done: i64,
    /// The tokens their runs used.
    pub tokens: i64,
    /// Tokens per done point, rounded down; none while no point is done.
    pub tokens_per_point: Option<i64>,
    /// The cards that are not done, archived ones left out.
    pub open: i64,
}

/// What was done on one day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThroughputDay {
    pub day: String,
    pub done: i64,
    pub points: i64,
}

/// How long something took, over a number of cards, in seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spread {
    pub cards: i64,
    pub median_seconds: i64,
    pub mean_seconds: i64,
    /// The nearest-rank 90th percentile.
    pub p90_seconds: i64,
}

/// The time the cards spent in a column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnTime {
    pub column: String,
    /// The cards that spent time in it.
    pub cards: i64,
    pub total_seconds: i64,
    /// The time each of those cards spent in it, on average.
    pub mean_seconds: i64,
}

/// The figures of one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFigures {
    pub id: i64,
    pub name: String,
    pub key: String,
    pub done: i64,
    pub points_done: i64,
    pub tokens: i64,
    pub tokens_per_point: Option<i64>,
}

/// The burn-up of a cycle or a milestone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BurnUp {
    pub id: i64,
    pub name: String,
    /// The project of a cycle.
    pub project: Option<String>,
    /// The day the cycle starts or the milestone was made.
    pub starts_on: String,
    /// The day the cycle ends or the milestone is due.
    pub ends_on: String,
    pub cards: i64,
    pub done: i64,
    pub points: i64,
    pub points_done: i64,
    pub days: Vec<BurnUpDay>,
}

fn day_format() -> Vec<format_description::BorrowedFormatItem<'static>> {
    format_description::parse_borrowed::<2>("[year]-[month]-[day]").expect("a valid format")
}

/// The median, the mean and the nearest-rank 90th percentile of `seconds`; none for no card.
fn spread(mut seconds: Vec<i64>) -> Option<Spread> {
    if seconds.is_empty() {
        return None;
    }
    seconds.sort_unstable();
    let n = seconds.len();
    let median = if n % 2 == 1 {
        seconds[n / 2]
    } else {
        (seconds[n / 2 - 1] + seconds[n / 2]) / 2
    };
    Some(Spread {
        cards: n as i64,
        median_seconds: median,
        mean_seconds: seconds.iter().sum::<i64>() / n as i64,
        p90_seconds: seconds[(n * 9).div_ceil(10) - 1],
    })
}

fn per_point(tokens: i64, points: i64) -> Option<i64> {
    (points > 0).then(|| tokens / points)
}

/// Tokens per point of `cards`: the tokens of the cards that have an estimate over their points, so that a card with no
/// estimate does not add tokens to a ratio it adds no points to.
fn cost_of_a_point(cards: &[&AnalyticsCard]) -> Option<i64> {
    let estimated = cards.iter().filter(|c| c.estimate.is_some());
    per_point(
        estimated.clone().map(|c| c.tokens).sum(),
        estimated.map(|c| i64::from(c.estimate.unwrap_or(0))).sum(),
    )
}

/// The intervals a card sat in a column, as `(column, from, to)`: from its making to its first move, between its
/// moves, and from its last move to when it was done, archived or `now`.
fn sittings(card: &AnalyticsCard, now: i64) -> Vec<(&str, i64, i64)> {
    let end = card.done_at.or(card.archived_at).unwrap_or(now);
    let mut out = Vec::new();
    let mut column = card
        .moves
        .first()
        .map_or(card.column.as_str(), |m| m.from.as_str());
    let mut since = card.created_at;
    for moved in &card.moves {
        out.push((column, since, moved.at));
        column = &moved.to;
        since = moved.at;
    }
    out.push((column, since, end));
    out
}

impl App {
    /// The analytics of `query` as `who` sees them.
    pub fn analytics(&self, who: &Actor, query: &AnalyticsQuery) -> Result<Analytics> {
        if !(1..=DAYS_LIMIT).contains(&query.days) {
            return Err(AppError::Invalid(format!(
                "a window is 1 to {DAYS_LIMIT} days, not {}",
                query.days
            )));
        }
        let sight = self.sight(who)?;
        let now = crate::store::now();
        let start = self.store.window_start(query.days)?;
        let today = self.store.today()?;
        let filter = CardFilter {
            repository: query.repository,
            agent: query.agent.clone().filter(|agent| !agent.is_empty()),
        };
        let narrowed = filter != CardFilter::default();
        let cards: Vec<AnalyticsCard> = self
            .store
            .analytics_cards(&filter)?
            .into_iter()
            .filter(|c| sight.project(c.project_id))
            .collect();
        let by_id: HashMap<i64, &AnalyticsCard> = cards.iter().map(|c| (c.id, c)).collect();
        let finished: Vec<&AnalyticsCard> = cards
            .iter()
            .filter(|c| c.done_at.is_some_and(|at| at >= start))
            .collect();
        let points = |c: &AnalyticsCard| i64::from(c.estimate.unwrap_or(0));

        let totals = {
            let tokens: i64 = finished.iter().map(|c| c.tokens).sum();
            let points_done: i64 = finished.iter().map(|c| points(c)).sum();
            AnalyticsTotals {
                done: finished.len() as i64,
                points_done,
                tokens,
                tokens_per_point: cost_of_a_point(&finished),
                open: cards
                    .iter()
                    .filter(|c| !c.done && c.archived_at.is_none())
                    .count() as i64,
            }
        };

        let mut throughput = Vec::new();
        if let Ok(last) = Date::parse(&today, &day_format()) {
            for back in (0..query.days).rev() {
                let day = (last - Duration::days(back)).to_string();
                let of: Vec<_> = finished
                    .iter()
                    .filter(|c| c.done_day.as_deref() == Some(day.as_str()))
                    .collect();
                throughput.push(ThroughputDay {
                    day,
                    done: of.len() as i64,
                    points: of.iter().map(|c| points(c)).sum(),
                });
            }
        }

        let cycle_time = spread(
            finished
                .iter()
                .filter_map(|c| Some(c.done_at? - c.started_at?).filter(|s| *s >= 0))
                .collect(),
        );
        let lead_time = spread(
            finished
                .iter()
                .filter_map(|c| Some((c.done_at? - c.created_at).max(0)))
                .collect(),
        );

        let mut spent: HashMap<&str, (i64, i64)> = HashMap::new();
        for card in &cards {
            let mut seen: HashMap<&str, i64> = HashMap::new();
            for (column, from, to) in sittings(card, now) {
                if self.column_class(card.project_id, column) != DONE && to >= start {
                    *seen.entry(column).or_default() += (to - from.max(start)).max(0);
                }
            }
            for (column, seconds) in seen {
                let entry = spent.entry(column).or_default();
                entry.0 += 1;
                entry.1 += seconds;
            }
        }
        let mut columns: Vec<ColumnTime> = spent
            .into_iter()
            .map(|(column, (cards, total))| ColumnTime {
                column: column.to_string(),
                cards,
                total_seconds: total,
                mean_seconds: total / cards,
            })
            .collect();
        columns.sort_by(|a, b| {
            b.mean_seconds
                .cmp(&a.mean_seconds)
                .then_with(|| a.column.cmp(&b.column))
        });

        let projects = self.store.projects()?;
        let project_rows = projects
            .iter()
            .filter(|p| cards.iter().any(|c| c.project_id == p.id))
            .map(|p| {
                let of: Vec<_> = finished.iter().filter(|c| c.project_id == p.id).collect();
                let tokens: i64 = of.iter().map(|c| c.tokens).sum();
                let points_done: i64 = of.iter().map(|c| points(c)).sum();
                ProjectFigures {
                    id: p.id,
                    name: p.name.clone(),
                    key: p.key.clone(),
                    done: of.len() as i64,
                    points_done,
                    tokens,
                    tokens_per_point: cost_of_a_point(&of.iter().map(|c| **c).collect::<Vec<_>>()),
                }
            })
            .collect();

        let burn = |id,
                    name,
                    project,
                    starts_on: &str,
                    ends_on: &str,
                    closed: Option<i64>,
                    members: Vec<(&AnalyticsCard, i64)>| {
            let planned: Vec<Planned> = members
                .iter()
                .map(|(c, added_at)| Planned {
                    added_at: *added_at,
                    points: points(c),
                    done_at: c.done_at,
                })
                .collect();
            let days = burn_up_between(starts_on, ends_on, closed, &planned, now);
            BurnUp {
                id,
                name,
                project,
                starts_on: starts_on.to_string(),
                ends_on: ends_on.to_string(),
                cards: members.len() as i64,
                done: members.iter().filter(|(c, _)| c.done).count() as i64,
                points: members.iter().map(|(c, _)| points(c)).sum(),
                points_done: members
                    .iter()
                    .filter(|(c, _)| c.done)
                    .map(|(c, _)| points(c))
                    .sum(),
                days,
            }
        };

        let mut in_cycle: HashMap<i64, Vec<(&AnalyticsCard, i64)>> = HashMap::new();
        for (cycle, card, added_at) in self.store.cycle_members()? {
            if let Some(card) = by_id.get(&card) {
                in_cycle.entry(cycle).or_default().push((card, added_at));
            }
        }
        let mut cycles = Vec::new();
        for project in projects.iter().filter(|p| sight.project(p.id)) {
            for cycle in self.store.cycles(project.id)? {
                let members = in_cycle.remove(&cycle.id).unwrap_or_default();
                if narrowed && members.is_empty() {
                    continue;
                }
                cycles.push(burn(
                    cycle.id,
                    cycle.name.clone(),
                    Some(project.name.clone()),
                    &cycle.starts_on,
                    &cycle.ends_on,
                    cycle.closed_at,
                    members,
                ));
            }
        }
        cycles.sort_by(|a, b| b.starts_on.cmp(&a.starts_on).then(b.id.cmp(&a.id)));
        cycles.truncate(BURN_UP_LIMIT);

        let mut in_milestone: HashMap<i64, Vec<&AnalyticsCard>> = HashMap::new();
        for (milestone, card) in self.store.milestone_members()? {
            if let Some(card) = by_id.get(&card) {
                in_milestone.entry(milestone).or_default().push(card);
            }
        }
        let mut milestones = Vec::new();
        for milestone in self.store.milestones()? {
            let members = in_milestone.remove(&milestone.id).unwrap_or_default();
            if members.is_empty() {
                continue;
            }
            let made = day_of(milestone.created_at).to_string();
            // The cards a milestone held on earlier days are not recorded: it counts the cards it holds now from the
            // day it was made.
            let members = members
                .into_iter()
                .map(|c| (c, milestone.created_at))
                .collect();
            milestones.push(burn(
                milestone.id,
                milestone.name,
                None,
                &made,
                // A milestone past its target date keeps burning up while cards finish.
                milestone.target_date.as_str().max(today.as_str()),
                None,
                members,
            ));
        }
        milestones.sort_by(|a, b| b.ends_on.cmp(&a.ends_on).then(b.id.cmp(&a.id)));
        milestones.truncate(BURN_UP_LIMIT);

        Ok(Analytics {
            today,
            days: query.days,
            totals,
            throughput,
            cycle_time,
            lead_time,
            columns,
            projects: project_rows,
            cycles,
            milestones,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration as Wait, Instant};

    use moebius_plugin_api::Host;

    use super::*;
    use crate::app::CycleFields;
    use crate::store::{Role, Store};

    /// The most seconds the analytics of ten thousand cards may take, a limit a slow machine in a debug build keeps.
    const TEN_THOUSAND_CARDS_SECONDS: u64 = 30;

    fn app() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Wait::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        (dir, app)
    }

    fn query(days: i64) -> AnalyticsQuery {
        AnalyticsQuery {
            days,
            ..AnalyticsQuery::default()
        }
    }

    /// Makes a card of `repository` with a made-up history: when it was made, what it was estimated at and the moves
    /// `(from, to, at)` it went through. A card that ended in `Done` is in it.
    fn card(
        app: &App,
        repository: i64,
        title: &str,
        created_at: i64,
        points: Option<u32>,
        moves: &[(&str, &str, i64)],
    ) -> i64 {
        let card = app
            .store
            .add_card(repository, title, "", "person", "Backlog")
            .unwrap();
        let mut sql = format!(
            "UPDATE cards SET created_at = {created_at} WHERE id = {};",
            card.id
        );
        if let Some(points) = points {
            sql += &format!(
                "INSERT INTO card_labels (card_id, label) VALUES ({}, 'estimate:{points}');",
                card.id
            );
        }
        for (from, to, at) in moves {
            sql += &format!(
                "INSERT INTO card_moves (card_id, from_column, to_column, at) VALUES ({}, '{from}', '{to}', {at});\
                 UPDATE cards SET column_name = '{to}', moved_at = {at} WHERE id = {};",
                card.id, card.id
            );
        }
        app.store.exec_for_test(&sql).unwrap();
        card.id
    }

    /// Tokens used by a run of `agent` on `card`.
    fn spend(app: &App, card: i64, agent: &str, purpose: &str, tokens: i64) {
        let run = app
            .store
            .add_run(Some(card), agent, "fake", purpose)
            .unwrap();
        app.store
            .add_usage(&crate::store::Usage {
                run_id: Some(run.id),
                model: "m".into(),
                input_tokens: tokens - 10,
                output_tokens: 10,
                cached_tokens: 0,
                duration_ms: 1,
                tools: vec![],
            })
            .unwrap();
    }

    #[test]
    fn the_numbers_of_a_made_up_history_are_right() {
        let (_dir, app) = app();
        let web = app
            .store
            .add_repository("web", "/w", "none", "main")
            .unwrap();
        let api = app
            .store
            .add_repository("api", "/a", "none", "main")
            .unwrap();
        let who = Actor::first_owner();
        // A window of three days starts at `s`; the day of `s + 12h` is two days ago, that of `s + 36h` yesterday.
        let s = app.store.window_start(3).unwrap();
        let (hour, day) = (3600, 86_400);

        let a = card(
            &app,
            web.id,
            "A",
            s - 100_000,
            Some(3),
            &[
                ("Backlog", "In progress", s + hour),
                ("In progress", "Done", s + 12 * hour),
            ],
        );
        let b = card(
            &app,
            web.id,
            "B",
            s,
            Some(5),
            &[
                ("Backlog", "Ready", s + 1000),
                ("Ready", "In progress", s + 4000),
                ("In progress", "Done", s + 36 * hour),
            ],
        );
        // Done before the window: in no figure of it.
        card(
            &app,
            web.id,
            "C",
            s - 2 * day,
            Some(8),
            &[("Backlog", "Done", s - hour)],
        );
        // Not done.
        let d = card(&app, web.id, "D", s - 5000, None, &[]);
        let f = card(
            &app,
            api.id,
            "F",
            s,
            Some(2),
            &[("Backlog", "Done", s + 12 * hour)],
        );
        spend(&app, a, "coder", "code", 300);
        spend(&app, b, "coder", "code", 400);
        spend(&app, b, "reviewer", "review", 300);
        spend(&app, b, "coder", "trial", 9999);

        let today = app.store.today().unwrap();
        let cycle = app
            .add_cycle(
                &who,
                web.home_project_id,
                &CycleFields {
                    name: "One".into(),
                    starts_on: (Date::parse(&today, &day_format()).unwrap() - Duration::days(2))
                        .to_string(),
                    ends_on: (Date::parse(&today, &day_format()).unwrap() + Duration::days(4))
                        .to_string(),
                },
            )
            .unwrap();
        for id in [a, b] {
            app.set_card_cycle(&who, id, Some(cycle.id)).unwrap();
        }
        let launch = app
            .store
            .add_milestone(
                "Launch",
                &(Date::parse(&today, &day_format()).unwrap() + Duration::days(10)).to_string(),
                "",
            )
            .unwrap();
        for id in [a, b, d] {
            app.store.set_card_milestones(id, &[launch.id]).unwrap();
        }

        let all = app.analytics(&who, &query(3)).unwrap();
        assert_eq!(
            all.totals,
            AnalyticsTotals {
                done: 3,
                points_done: 10,
                tokens: 1000,
                tokens_per_point: Some(100),
                open: 1
            }
        );
        let throughput: Vec<(i64, i64)> =
            all.throughput.iter().map(|d| (d.done, d.points)).collect();
        assert_eq!(throughput, [(2, 5), (1, 5), (0, 0)]);
        assert_eq!(all.throughput[2].day, today);
        // A and B were started; F was not.
        assert_eq!(
            all.cycle_time,
            Some(Spread {
                cards: 2,
                median_seconds: 82_600,
                mean_seconds: 82_600,
                p90_seconds: 125_600
            })
        );
        assert_eq!(
            all.lead_time,
            Some(Spread {
                cards: 3,
                median_seconds: 129_600,
                mean_seconds: 105_333,
                p90_seconds: 143_200
            })
        );
        let column = |name: &str| {
            all.columns
                .iter()
                .find(|c| c.column == name)
                .unwrap()
                .clone()
        };
        assert_eq!(
            column("In progress"),
            ColumnTime {
                column: "In progress".into(),
                cards: 2,
                total_seconds: 165_200,
                mean_seconds: 82_600
            }
        );
        assert_eq!(
            column("Ready"),
            ColumnTime {
                column: "Ready".into(),
                cards: 1,
                total_seconds: 3000,
                mean_seconds: 3000
            }
        );
        // A, B, D and F sat in Backlog (C left it before the window); D's time is still running.
        assert_eq!(column("Backlog").cards, 4);
        assert!(column("Backlog").total_seconds >= 3600 + 1000 + 12 * hour);
        assert!(all.columns.iter().all(|c| c.column != "Done"));
        assert_eq!(all.columns[0].column, "In progress");

        assert_eq!(all.projects.len(), 2);
        let ppt = |name: &str| {
            all.projects
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .tokens_per_point
        };
        assert_eq!(ppt(&web.name), Some(125));
        assert_eq!(ppt(&api.name), Some(0));

        let cycle = &all.cycles[0];
        assert_eq!(
            (cycle.cards, cycle.done, cycle.points, cycle.points_done),
            (2, 2, 8, 8)
        );
        let last = cycle.days.last().unwrap();
        assert_eq!((last.scope, last.done), (8, 8));
        assert_eq!(cycle.days[0].scope, 0);
        let launch = &all.milestones[0];
        assert_eq!(
            (launch.cards, launch.done, launch.points, launch.points_done),
            (3, 2, 8, 8)
        );
        assert_eq!(launch.days.len(), 1);
        assert_eq!((launch.days[0].scope, launch.days[0].done), (8, 8));

        // A longer window takes C in; a window of one day only what is done today.
        assert_eq!(app.analytics(&who, &query(30)).unwrap().totals.done, 4);
        assert_eq!(app.analytics(&who, &query(1)).unwrap().totals.done, 0);

        // The filters narrow every figure.
        let narrowed = |repository, agent: Option<&str>| {
            app.analytics(
                &who,
                &AnalyticsQuery {
                    days: 3,
                    repository,
                    agent: agent.map(String::from),
                },
            )
            .unwrap()
        };
        let api_only = narrowed(Some(api.id), None);
        assert_eq!(
            (
                api_only.totals.done,
                api_only.totals.points_done,
                api_only.totals.tokens
            ),
            (1, 2, 0)
        );
        assert_eq!(api_only.projects.len(), 1);
        assert!(api_only.cycles.is_empty() && api_only.milestones.is_empty());
        assert_eq!(api_only.lead_time.unwrap().cards, 1);
        assert!(api_only.cycle_time.is_none());
        let web_only = narrowed(Some(web.id), None);
        assert_eq!((web_only.totals.done, web_only.totals.tokens), (2, 1000));
        let reviewer = narrowed(None, Some("reviewer"));
        assert_eq!(
            reviewer.totals,
            AnalyticsTotals {
                done: 1,
                points_done: 5,
                tokens: 300,
                tokens_per_point: Some(60),
                open: 0
            }
        );
        assert_eq!(reviewer.cycles[0].cards, 1);
        assert_eq!(reviewer.milestones[0].points, 5);
        let coder = narrowed(None, Some("coder"));
        assert_eq!((coder.totals.done, coder.totals.tokens), (2, 700));
        let none = narrowed(None, Some("nobody"));
        assert_eq!(
            (
                none.totals.done,
                none.throughput.iter().map(|d| d.done).sum::<i64>()
            ),
            (0, 0)
        );
        assert!(none.cycles.is_empty() && none.columns.is_empty() && none.projects.is_empty());
        let both = narrowed(Some(api.id), Some("coder"));
        assert_eq!(both.totals.done, 0);
        let _ = f;

        // Archived cards stay in.
        app.store
            .exec_for_test(&format!(
                "UPDATE cards SET archived_at = 1 WHERE id IN ({a}, {b}, {d})"
            ))
            .unwrap();
        let archived = app.analytics(&who, &query(3)).unwrap();
        assert_eq!(archived.totals.done, 3);
        assert_eq!(archived.totals.open, 0);
        assert_eq!(archived.totals.tokens, 1000);
    }

    #[test]
    fn a_private_project_is_in_no_figure_of_a_person_who_is_not_its_member() {
        let (_dir, app) = app();
        let open = app
            .store
            .add_repository("open", "/o", "none", "main")
            .unwrap();
        let secret = app
            .store
            .add_repository("secret", "/s", "none", "main")
            .unwrap();
        let lea = app.store.add_person("lea", "Lea", Role::Member).unwrap();
        let ben = app.store.add_person("ben", "Ben", Role::Member).unwrap();
        app.store
            .set_project_access(secret.home_project_id, true, &[lea.id])
            .unwrap();
        let s = app.store.window_start(3).unwrap();
        let open_card = card(
            &app,
            open.id,
            "Open",
            s,
            Some(1),
            &[("Backlog", "Done", s + 100)],
        );
        let hidden = card(
            &app,
            secret.id,
            "Hidden",
            s,
            Some(4),
            &[("Backlog", "Done", s + 100)],
        );
        spend(&app, open_card, "coder", "code", 10);
        spend(&app, hidden, "coder", "code", 1000);
        let milestone = app.store.add_milestone("M", "2099-01-01", "").unwrap();
        app.store
            .set_card_milestones(hidden, &[milestone.id])
            .unwrap();
        let cycle = app
            .store
            .add_cycle(
                secret.home_project_id,
                "Secret cycle",
                "2026-11-02",
                "2026-11-15",
            )
            .unwrap();
        app.store.set_card_cycle(hidden, Some(cycle.id)).unwrap();

        let figures = |who: Actor| app.analytics(&who, &query(3)).unwrap();
        let (ben, lea) = (
            figures(Actor::person(ben.id, "ben")),
            figures(Actor::person(lea.id, "lea")),
        );
        assert_eq!(
            (ben.totals.done, ben.totals.points_done, ben.totals.tokens),
            (1, 1, 10)
        );
        assert_eq!(ben.projects.len(), 1);
        assert!(ben.cycles.is_empty() && ben.milestones.is_empty());
        assert_eq!(ben.throughput.iter().map(|d| d.done).sum::<i64>(), 1);
        assert_eq!(
            (lea.totals.done, lea.totals.points_done, lea.totals.tokens),
            (2, 5, 1010)
        );
        assert_eq!(
            (lea.cycles.len(), lea.milestones.len(), lea.projects.len()),
            (1, 1, 2)
        );
        // An owner who is no member sees nothing private either; a column script sees everything.
        assert_eq!(figures(Actor::first_owner()).totals.done, 1);
        assert_eq!(
            figures(Actor::Script {
                column: "Done".into()
            })
            .totals
            .done,
            2
        );
    }

    #[test]
    fn a_window_outside_one_to_a_year_is_refused() {
        let (_dir, app) = app();
        for days in [0, -1, 366] {
            assert!(matches!(
                app.analytics(&Actor::first_owner(), &query(days)),
                Err(AppError::Invalid(_))
            ));
        }
        assert!(app.analytics(&Actor::first_owner(), &query(365)).is_ok());
    }

    #[test]
    fn the_analytics_of_ten_thousand_cards_come_within_a_generous_time() {
        let (_dir, app) = app();
        let repository = app
            .store
            .add_repository("big", "/b", "none", "main")
            .unwrap();
        let cycle = app
            .store
            .add_cycle(
                repository.home_project_id,
                "Big",
                "2026-01-01",
                "2026-12-31",
            )
            .unwrap();
        let s = app.store.window_start(30).unwrap();
        for n in 0..10_000 {
            app.store
                .add_card(repository.id, &format!("Card {n}"), "", "person", "Backlog")
                .unwrap();
        }
        app.store
            .exec_for_test(&format!(
                "UPDATE cards SET created_at = {s} + id, column_name = 'Done', moved_at = {s} + 100 + id * 20;
                 INSERT INTO card_moves (card_id, from_column, to_column, at) SELECT id, 'Backlog', 'In progress', {s} + 50 + id FROM cards;
                 INSERT INTO card_moves (card_id, from_column, to_column, at) SELECT id, 'In progress', 'Done', {s} + 100 + id * 20 FROM cards;
                 INSERT INTO card_labels (card_id, label) SELECT id, 'estimate:' || (1 + id % 5) FROM cards;
                 INSERT INTO runs (card_id, agent, harness, purpose, status, started_at) SELECT id, 'coder', 't', 'code', 'done', {s} + 60 + id FROM cards;
                 INSERT INTO usage (run_id, model, input_tokens, output_tokens, cached_tokens, at) SELECT id, 'm', 100, 20, 0, started_at FROM runs;
                 INSERT INTO cycle_cards (cycle_id, card_id, added_at) SELECT {}, id, {s} FROM cards;",
                cycle.id
            ))
            .unwrap();
        let started = Instant::now();
        let answer = app.analytics(&Actor::first_owner(), &query(30)).unwrap();
        let took = started.elapsed();
        assert!(
            took.as_secs() < TEN_THOUSAND_CARDS_SECONDS,
            "took {took:?}, more than {TEN_THOUSAND_CARDS_SECONDS} seconds"
        );
        assert_eq!(answer.totals.done, 10_000);
        assert_eq!(answer.totals.tokens, 1_200_000);
        assert_eq!(answer.cycles[0].cards, 10_000);
    }
}
