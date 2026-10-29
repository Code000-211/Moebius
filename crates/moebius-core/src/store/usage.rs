//! Usage: the tokens and time of every model call, and what they add up to by card, day, model, agent, activity and
//! tool.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::{Result, Store, now};

/// The tool names of a `usage.tools` column; none for a row recorded before the column or one that does not read.
fn tools_from_column(column: Option<String>) -> Vec<String> {
    column
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub run_id: Option<i64>,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_tokens: i64,
    /// How long the model call took, from the request to the end of the response or stream.
    pub duration_ms: i64,
    /// The names of the tools the answer asked for, in order; empty for an answer that asks for none.
    pub tools: Vec<String>,
}

/// The name under which usage that belongs to no run is counted by agent.
pub const NO_RUN: &str = "No run";

/// What one model used in a window of days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub tokens: i64,
    pub calls: i64,
    /// The input tokens among `tokens`, which `cached_tokens` is a share of.
    pub input_tokens: i64,
    pub cached_tokens: i64,
}

/// What one model used when it was asked by a class, in a window of days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassUsage {
    pub class: String,
    pub model: String,
    pub tokens: i64,
    pub calls: i64,
}

/// What the runs of one agent used in a window of days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentUsage {
    pub agent: String,
    pub calls: i64,
    pub tokens: i64,
    pub input_tokens: i64,
    pub cached_tokens: i64,
    pub model_ms: i64,
}

/// What the model did with its time in a window of days: one tool, answering, or something unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityUsage {
    /// A tool's name, [`ANSWERING`] or [`UNKNOWN`].
    pub activity: String,
    pub model_ms: i64,
    pub calls: i64,
    pub output_tokens: i64,
}

/// The activity of a model call that asked for no tool.
pub const ANSWERING: &str = "answering";
/// The activity of a model call recorded before the tools of calls were.
pub const UNKNOWN: &str = "unknown";

/// What a tool made runs wait for in a window of days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolWait {
    pub tool: String,
    pub calls: i64,
    pub seconds: i64,
}

/// What making tool outputs smaller saved in a window of days: how many results were, and their bytes before and
/// after (the `tool_output` run events).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolOutputSaving {
    pub results: i64,
    pub bytes_before: i64,
    pub bytes_after: i64,
}

/// All usage in a window of days: tokens, calls, input tokens, cached tokens and model time.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsageTotals {
    pub tokens: i64,
    pub calls: i64,
    pub input_tokens: i64,
    pub cached_tokens: i64,
    pub model_ms: i64,
}

/// The start of the local day that is `?1 - 1` days ago, as Unix seconds: the first moment of a window of `?1` days.
const WINDOW_START: &str = "CAST(strftime('%s', 'now', 'localtime', 'start of day', '-' || (?1 - 1) || ' days', 'utc') AS INTEGER)";

impl Usage {
    pub fn total(&self) -> i64 {
        self.input_tokens + self.output_tokens
    }
}

impl Store {
    pub fn add_usage(&self, u: &Usage) -> Result<()> {
        self.add_class_usage(u, None)
    }

    /// Records usage, with the model class the request named when it named one (see `router::classes`).
    pub fn add_class_usage(&self, u: &Usage, class: Option<&str>) -> Result<()> {
        self.conn().execute(
            "INSERT INTO usage (run_id, model, input_tokens, output_tokens, cached_tokens, duration_ms, tools, at, class) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![u.run_id, u.model, u.input_tokens, u.output_tokens, u.cached_tokens, u.duration_ms, serde_json::to_string(&u.tools)?, now(), class],
        )?;
        Ok(())
    }

    /// Tokens `model` used since the start of the server's local day, whatever asked for it.
    pub fn model_tokens_today(&self, model: &str) -> Result<i64> {
        Ok(self.conn().query_row(
            &format!(
                "SELECT COALESCE(SUM(input_tokens + output_tokens), 0) FROM usage
                 WHERE at >= {WINDOW_START} AND model = ?2"
            ),
            params![1, model],
            |r| r.get(0),
        )?)
    }

    /// What each model used when asked by each class in the last `days` days, by class and then the model with the
    /// most tokens first. Usage asked for by a model's own name is not here.
    pub fn usage_by_class(&self, days: i64) -> Result<Vec<ClassUsage>> {
        self.rows(
            &format!(
                "SELECT class, model, SUM(input_tokens + output_tokens) AS tokens, COUNT(*)
             FROM usage WHERE at >= {WINDOW_START} AND class IS NOT NULL
             GROUP BY class, model ORDER BY class, tokens DESC, model"
            ),
            [days],
            |r| {
                Ok(ClassUsage {
                    class: r.get(0)?,
                    model: r.get(1)?,
                    tokens: r.get(2)?,
                    calls: r.get(3)?,
                })
            },
        )
    }

    pub fn usage_for_run(&self, run_id: i64) -> Result<Vec<Usage>> {
        self.rows("SELECT run_id, model, input_tokens, output_tokens, cached_tokens, duration_ms, tools FROM usage WHERE run_id = ?1 ORDER BY id", [run_id], |r| {
            Ok(Usage {
                run_id: r.get(0)?,
                model: r.get(1)?,
                input_tokens: r.get(2)?,
                output_tokens: r.get(3)?,
                cached_tokens: r.get(4)?,
                duration_ms: r.get(5)?,
                tools: tools_from_column(r.get(6)?),
            })
        })
    }

    /// Tokens used by the runs of a card since it was last retried.
    pub fn tokens_for_card(&self, card_id: i64) -> Result<i64> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(SUM(u.input_tokens + u.output_tokens), 0) FROM usage u JOIN runs r ON r.id = u.run_id
             WHERE r.card_id = ?1 AND r.purpose <> 'trial'
               AND r.id > COALESCE((SELECT retried_after FROM cards WHERE id = ?1), 0)",
            [card_id],
            |r| r.get(0),
        )?)
    }

    /// Tokens used by all runs since the start of the server's local day.
    pub fn tokens_today(&self) -> Result<i64> {
        Ok(self.usage_totals(1)?.tokens)
    }

    /// Everything used in the last `days` local days, today included.
    pub fn usage_totals(&self, days: i64) -> Result<UsageTotals> {
        Ok(self.conn().query_row(
            &format!(
                "SELECT COALESCE(SUM(input_tokens + output_tokens), 0), COUNT(*), COALESCE(SUM(input_tokens), 0),
                        COALESCE(SUM(cached_tokens), 0), COALESCE(SUM(duration_ms), 0)
                 FROM usage WHERE at >= {WINDOW_START}"
            ),
            [days],
            |r| {
                Ok(UsageTotals {
                    tokens: r.get(0)?,
                    calls: r.get(1)?,
                    input_tokens: r.get(2)?,
                    cached_tokens: r.get(3)?,
                    model_ms: r.get(4)?,
                })
            },
        )?)
    }

    /// The tokens of each model on each local day of the last `days` days, as `(YYYY-MM-DD, model, tokens)`,
    /// oldest day first.
    pub fn usage_by_day(&self, days: i64) -> Result<Vec<(String, String, i64)>> {
        self.rows(&format!(
            "SELECT strftime('%Y-%m-%d', at, 'unixepoch', 'localtime') AS day, model, SUM(input_tokens + output_tokens)
             FROM usage WHERE at >= {WINDOW_START} GROUP BY day, model ORDER BY day, model"
        ), [days], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
    }

    /// What each model used in the last `days` days, the one with the most tokens first.
    pub fn usage_by_model(&self, days: i64) -> Result<Vec<ModelUsage>> {
        self.rows(&format!(
            "SELECT model, SUM(input_tokens + output_tokens) AS tokens, COUNT(*), SUM(input_tokens), SUM(cached_tokens)
             FROM usage WHERE at >= {WINDOW_START} GROUP BY model ORDER BY tokens DESC, model"
        ), [days], |r| {
            Ok(ModelUsage {
                model: r.get(0)?,
                tokens: r.get(1)?,
                calls: r.get(2)?,
                input_tokens: r.get(3)?,
                cached_tokens: r.get(4)?,
            })
        })
    }

    /// What the runs of each agent used in the last `days` days, the agent with the most tokens first. Usage without
    /// a run is counted under [`NO_RUN`].
    pub fn usage_by_agent(&self, days: i64) -> Result<Vec<AgentUsage>> {
        self.rows(&format!(
            "SELECT COALESCE(r.agent, ?2) AS agent, COUNT(*), SUM(u.input_tokens + u.output_tokens) AS tokens,
                    SUM(u.input_tokens), SUM(u.cached_tokens), SUM(u.duration_ms)
             FROM usage u LEFT JOIN runs r ON r.id = u.run_id
             WHERE u.at >= {WINDOW_START} GROUP BY agent ORDER BY tokens DESC, agent"
        ), params![days, NO_RUN], |r| {
            Ok(AgentUsage {
                agent: r.get(0)?,
                calls: r.get(1)?,
                tokens: r.get(2)?,
                input_tokens: r.get(3)?,
                cached_tokens: r.get(4)?,
                model_ms: r.get(5)?,
            })
        })
    }

    /// What the model did in the last `days` days, the activity with the most model time first. A call that asked for
    /// `n` tools gives each tool `1/n` of its duration and output tokens (what does not divide goes to the first
    /// tools, so nothing is lost); a call that asked for none is `answering`. A call counts once for each distinct
    /// tool it named. Calls recorded before their tools were are `unknown`, and are listed only when there are no other
    /// calls in the window.
    pub fn usage_by_activity(&self, days: i64) -> Result<Vec<ActivityUsage>> {
        let calls: Vec<(Option<String>, i64, i64)> = self.rows(
            &format!(
                "SELECT tools, duration_ms, output_tokens FROM usage WHERE at >= {WINDOW_START}"
            ),
            [days],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let mut sums: BTreeMap<String, ActivityUsage> = BTreeMap::new();
        let mut unknown = ActivityUsage {
            activity: UNKNOWN.into(),
            model_ms: 0,
            calls: 0,
            output_tokens: 0,
        };
        for (tools, model_ms, output_tokens) in calls {
            let Some(tools) = tools else {
                unknown.model_ms += model_ms;
                unknown.calls += 1;
                unknown.output_tokens += output_tokens;
                continue;
            };
            let tools = tools_from_column(Some(tools));
            let names: Vec<&str> = if tools.is_empty() {
                vec![ANSWERING]
            } else {
                tools.iter().map(String::as_str).collect()
            };
            let n = names.len() as i64;
            let mut counted = BTreeSet::new();
            for (i, name) in names.into_iter().enumerate() {
                let i = i as i64;
                let sum = sums
                    .entry(name.to_string())
                    .or_insert_with(|| ActivityUsage {
                        activity: name.to_string(),
                        model_ms: 0,
                        calls: 0,
                        output_tokens: 0,
                    });
                sum.model_ms += model_ms / n + i64::from(i < model_ms % n);
                sum.output_tokens += output_tokens / n + i64::from(i < output_tokens % n);
                if counted.insert(name) {
                    sum.calls += 1;
                }
            }
        }
        let mut activities: Vec<ActivityUsage> = sums.into_values().collect();
        if activities.is_empty() && unknown.calls > 0 {
            activities.push(unknown);
        }
        activities.sort_by(|a, b| {
            b.model_ms
                .cmp(&a.model_ms)
                .then_with(|| a.activity.cmp(&b.activity))
        });
        Ok(activities)
    }

    /// The tools whose calls took longest in the last `days` days, by the time between a call's
    /// `tool_execution_start` and `tool_execution_end` events, at most `limit` of them.
    pub fn tool_waits(&self, days: i64, limit: i64) -> Result<Vec<ToolWait>> {
        self.rows(&format!(
            "SELECT json_extract(s.data, '$.toolName') AS tool, COUNT(*), SUM(e.at - s.at) AS seconds
             FROM events s JOIN events e ON e.run_id = s.run_id AND e.kind = 'tool_execution_end'
              AND json_extract(e.data, '$.toolCallId') = json_extract(s.data, '$.toolCallId')
             WHERE s.kind = 'tool_execution_start' AND s.at >= {WINDOW_START} AND tool IS NOT NULL
             GROUP BY tool ORDER BY seconds DESC, tool LIMIT ?2"
        ), params![days, limit], |r| {
            Ok(ToolWait {
                tool: r.get(0)?,
                calls: r.get(1)?,
                seconds: r.get(2)?,
            })
        })
    }
}

impl Store {
    /// What making tool outputs smaller saved in the last `days` days.
    pub fn tool_output_saving(&self, days: i64) -> Result<ToolOutputSaving> {
        Ok(self.conn().query_row(
            &format!(
                "SELECT COUNT(*), COALESCE(SUM(json_extract(data, '$.bytes_before')), 0),
                        COALESCE(SUM(json_extract(data, '$.bytes_after')), 0)
                 FROM events WHERE kind = 'tool_output' AND at >= {WINDOW_START}"
            ),
            [days],
            |r| {
                Ok(ToolOutputSaving {
                    results: r.get(0)?,
                    bytes_before: r.get(1)?,
                    bytes_after: r.get(2)?,
                })
            },
        )?)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::store::{Run, test_store as store};

    #[test]
    fn a_retry_forgets_the_tokens_used_before_it_and_today_counts_only_today() {
        let (_dir, s) = store();
        let repo = s.add_repository("r", "/r", "none", "main").unwrap();
        let card = s.add_card(repo.id, "A", "", "leon", "Backlog").unwrap();
        let used = |run: &Run, tokens| {
            s.add_usage(&Usage {
                run_id: Some(run.id),
                model: "local".into(),
                input_tokens: tokens,
                output_tokens: 1,
                cached_tokens: 0,
                duration_ms: 0,
                tools: Vec::new(),
            })
            .unwrap();
        };
        let first = s.add_run(Some(card.id), "Coder", "pi", "code").unwrap();
        used(&first, 99);
        assert_eq!(s.tokens_for_card(card.id).unwrap(), 100);
        assert_eq!(s.card(card.id).unwrap().unwrap().retried_after, None);

        s.set_card_retried(card.id).unwrap();
        assert_eq!(
            s.card(card.id).unwrap().unwrap().retried_after,
            Some(first.id)
        );
        assert_eq!(s.tokens_for_card(card.id).unwrap(), 0);
        let second = s.add_run(Some(card.id), "Coder", "pi", "code").unwrap();
        used(&second, 9);
        assert_eq!(s.tokens_for_card(card.id).unwrap(), 10);

        // Usage from before the start of today is not today's.
        assert_eq!(s.tokens_today().unwrap(), 110);
        s.conn()
            .execute(
                "UPDATE usage SET at = at - 2 * 86400 WHERE run_id = ?1",
                [first.id],
            )
            .unwrap();
        assert_eq!(s.tokens_today().unwrap(), 10);
    }

    #[test]
    fn model_time_is_split_over_what_each_call_did() {
        let (_dir, s) = store();
        let add = |tools: &[&str], output_tokens, duration_ms| {
            s.add_usage(&Usage {
                run_id: None,
                model: "m".into(),
                input_tokens: 1,
                output_tokens,
                cached_tokens: 0,
                duration_ms,
                tools: tools.iter().map(|t| t.to_string()).collect(),
            })
            .unwrap();
        };
        add(&["bash"], 10, 1000);
        add(&["bash", "read"], 21, 3001);
        add(&["read", "read"], 4, 400);
        add(&[], 7, 500);
        // Two days ago, and before the tools of calls were recorded.
        add(&["edit"], 5, 9000);
        s.conn()
            .execute("UPDATE usage SET at = at - 2 * 86400 WHERE id = 5", [])
            .unwrap();
        let activity = |activity: &str, model_ms, calls, output_tokens| ActivityUsage {
            activity: activity.into(),
            model_ms,
            calls,
            output_tokens,
        };

        // The odd millisecond and token of the second call go to its first tool; a call that names a tool twice
        // counts once for it but gives it both shares.
        assert_eq!(
            s.usage_by_activity(1).unwrap(),
            [
                activity("bash", 2501, 2, 21),
                activity("read", 1900, 2, 14),
                activity(ANSWERING, 500, 1, 7),
            ]
        );
        assert_eq!(
            s.usage_by_activity(3).unwrap()[0],
            activity("edit", 9000, 1, 5)
        );

        // Calls from before the tools were recorded are left out while there are others, and listed alone otherwise.
        s.conn()
            .execute("UPDATE usage SET tools = NULL WHERE id = 1", [])
            .unwrap();
        assert_eq!(s.usage_by_activity(1).unwrap().len(), 3);
        assert!(
            !s.usage_by_activity(1)
                .unwrap()
                .iter()
                .any(|a| a.activity == UNKNOWN)
        );
        s.conn()
            .execute("UPDATE usage SET tools = NULL", [])
            .unwrap();
        assert_eq!(
            s.usage_by_activity(1).unwrap(),
            [activity(UNKNOWN, 4901, 4, 42)]
        );
    }

    #[test]
    fn usage_is_counted_by_window_day_model_agent_and_tool() {
        let (_dir, s) = store();
        let coder = s.add_run(None, "Coder", "pi", "code").unwrap().id;
        let reviewer = s.add_run(None, "Reviewer", "pi", "review").unwrap().id;
        let add = |run: Option<i64>, model: &str, input, output, cached, duration_ms| {
            s.add_usage(&Usage {
                run_id: run,
                model: model.into(),
                input_tokens: input,
                output_tokens: output,
                cached_tokens: cached,
                duration_ms,
                tools: Vec::new(),
            })
            .unwrap();
        };
        add(Some(coder), "big", 1000, 100, 600, 4000);
        add(Some(coder), "big", 500, 50, 0, 2000);
        add(Some(reviewer), "small", 200, 20, 100, 1000);
        add(None, "big", 10, 0, 0, 500);
        // Two days ago a smaller model led.
        add(Some(reviewer), "small", 900, 0, 0, 3000);
        add(Some(coder), "big", 100, 0, 0, 100);
        s.conn()
            .execute("UPDATE usage SET at = at - 2 * 86400 WHERE id > 4", [])
            .unwrap();
        let day = |ago: i64| -> String {
            s.conn()
                .query_row(
                    "SELECT strftime('%Y-%m-%d', 'now', 'localtime', ?1)",
                    [format!("-{ago} days")],
                    |r| r.get(0),
                )
                .unwrap()
        };

        assert_eq!(s.today().unwrap(), day(0));
        assert_eq!(
            s.usage_totals(1).unwrap(),
            UsageTotals {
                tokens: 1880,
                calls: 4,
                input_tokens: 1710,
                cached_tokens: 700,
                model_ms: 7500
            }
        );
        assert_eq!(s.tokens_today().unwrap(), 1880);
        let three = s.usage_totals(3).unwrap();
        assert_eq!(
            (three.tokens, three.calls, three.model_ms),
            (2880, 6, 10600)
        );

        let model = |model: &str, tokens, calls, input_tokens, cached_tokens| ModelUsage {
            model: model.into(),
            tokens,
            calls,
            input_tokens,
            cached_tokens,
        };
        assert_eq!(
            s.usage_by_model(1).unwrap(),
            [
                model("big", 1660, 3, 1510, 600),
                model("small", 220, 1, 200, 100)
            ]
        );
        assert_eq!(
            s.usage_by_model(3).unwrap(),
            [
                model("big", 1760, 4, 1610, 600),
                model("small", 1120, 2, 1100, 100)
            ]
        );

        let agent =
            |agent: &str, calls, tokens, input_tokens, cached_tokens, model_ms| AgentUsage {
                agent: agent.into(),
                calls,
                tokens,
                input_tokens,
                cached_tokens,
                model_ms,
            };
        assert_eq!(
            s.usage_by_agent(1).unwrap(),
            [
                agent("Coder", 2, 1650, 1500, 600, 6000),
                agent("Reviewer", 1, 220, 200, 100, 1000),
                agent(NO_RUN, 1, 10, 10, 0, 500),
            ]
        );

        let (old, today) = (day(2), day(0));
        let by_day =
            |date: &str, model: &str, tokens| (date.to_string(), model.to_string(), tokens);
        assert_eq!(
            s.usage_by_day(365).unwrap(),
            [
                by_day(&old, "big", 100),
                by_day(&old, "small", 900),
                by_day(&today, "big", 1660),
                by_day(&today, "small", 220),
            ]
        );
        assert_eq!(s.usage_by_day(1).unwrap().len(), 2);

        // A call waits from its start event to its end event; a call that has not ended waits for nothing yet.
        let call = |id: &str, tool: &str, seconds: i64| {
            let start = json!({"toolCallId": id, "toolName": tool});
            let begun = s
                .append_event(coder, "tool_execution_start", &start)
                .unwrap();
            let end = s.append_event(coder, "tool_execution_end", &start).unwrap();
            // The end is set from the start, so that a second passing between the two appends changes nothing.
            s.conn()
                .execute(
                    "UPDATE events SET at = (SELECT at FROM events WHERE run_id = ?2 AND seq = ?4) + ?1 \
                     WHERE run_id = ?2 AND seq = ?3",
                    params![seconds, coder, end, begun],
                )
                .unwrap();
        };
        call("c1", "bash", 60);
        call("c2", "bash", 30);
        call("c3", "edit", 5);
        call("c4", "read", 0);
        s.append_event(
            coder,
            "tool_execution_start",
            &json!({"toolCallId": "c5", "toolName": "grep"}),
        )
        .unwrap();
        let wait = |tool: &str, calls, seconds| ToolWait {
            tool: tool.into(),
            calls,
            seconds,
        };
        assert_eq!(
            s.tool_waits(1, 10).unwrap(),
            [wait("bash", 2, 90), wait("edit", 1, 5), wait("read", 1, 0)]
        );
        assert_eq!(s.tool_waits(1, 2).unwrap().len(), 2);

        // What made tool outputs smaller saved.
        assert_eq!(
            s.tool_output_saving(1).unwrap(),
            ToolOutputSaving::default()
        );
        for (before, after) in [(9000, 1000), (500, 400)] {
            let saved = json!({"bytes_before": before, "bytes_after": after});
            s.append_event(coder, "tool_output", &saved).unwrap();
        }
        assert_eq!(
            s.tool_output_saving(1).unwrap(),
            ToolOutputSaving {
                results: 2,
                bytes_before: 9500,
                bytes_after: 1400
            }
        );
    }

    #[test]
    fn usage_asked_by_a_class_is_counted_by_class_and_by_model_for_today() {
        let (_dir, s) = store();
        let add = |model: &str, class: Option<&str>, tokens| {
            let usage = Usage {
                run_id: None,
                model: model.into(),
                input_tokens: tokens,
                output_tokens: 0,
                cached_tokens: 0,
                duration_ms: 0,
                tools: Vec::new(),
            };
            s.add_class_usage(&usage, class).unwrap();
        };
        add("big", Some("strong"), 100);
        add("big", Some("strong"), 50);
        add("small", Some("strong"), 500);
        add("big", None, 7);
        add("small", Some("fast"), 1);
        add("big", Some("strong"), 1000);
        s.conn()
            .execute("UPDATE usage SET at = at - 2 * 86400 WHERE id = 6", [])
            .unwrap();
        let class = |class: &str, model: &str, tokens, calls| ClassUsage {
            class: class.into(),
            model: model.into(),
            tokens,
            calls,
        };
        assert_eq!(
            s.usage_by_class(1).unwrap(),
            [
                class("fast", "small", 1, 1),
                class("strong", "small", 500, 1),
                class("strong", "big", 150, 2),
            ]
        );
        assert_eq!(
            s.usage_by_class(3).unwrap()[1],
            class("strong", "big", 1150, 3)
        );
        // A model's tokens for today count whatever asked for it.
        assert_eq!(s.model_tokens_today("big").unwrap(), 157);
        assert_eq!(s.model_tokens_today("none").unwrap(), 0);
    }
}
