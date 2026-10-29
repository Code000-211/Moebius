//! The Usage page: the heatmap of tokens by day, the models and agents, the sidebar's meter, and the windows asked for.

use iced::Size;
use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client;
use moebius_desktop::screens::usage;
use serde_json::json;

use crate::fixtures::{clicked, draw_sized, overview, shows};

/// The day numbers of 1970-01-01 onwards as `YYYY-MM-DD` (Howard Hinnant's algorithm).
fn date_of(day: i64) -> String {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let of_era = z - era * 146_097;
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * of_year + 2) / 153;
    let day_of_month = of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day_of_month:02}")
}

/// The usage of the mockup: 135 days of tokens led by three models, four models today, five agents, four tools.
/// 2026-10-28 is the 20,389th day since 1970-01-01.
fn usage_report(day_budget: Option<i64>) -> client::UsageReport {
    const TODAY: i64 = 20_754;
    let leaders = ["Sonnet 5.5", "Opus 5.5", "Qwen3.8 Flash Next · local"];
    let days: Vec<_> = (0..135)
        .map(|i| {
            let day = TODAY - 134 + i;
            // Some days have no usage; the others get a size and a leading model from their number.
            let tokens = if i % 9 == 4 {
                0
            } else if day == TODAY {
                1_840_000
            } else {
                (80_000 + (i * 7_919 + i * i * 131) % 2_400_000).min(2_520_000)
            };
            let model = match i {
                0..3 => leaders[i as usize],
                _ => leaders[((i * 5 + i / 4) % 7 % 3) as usize],
            };
            json!({"date": date_of(day), "tokens": tokens, "model": model})
        })
        .filter(|day| day["tokens"] != 0)
        .collect();
    let model = |name: &str, tokens: i64, calls: i64, input: i64, cached: i64| json!({"model": name, "tokens": tokens, "calls": calls, "input_tokens": input, "cached_tokens": cached});
    let agent = |name: &str, calls: i64, tokens: i64, input: i64, cached: i64, ms: i64| {
        json!({"agent": name, "calls": calls, "tokens": tokens, "input_tokens": input,
               "cached_tokens": cached, "model_ms": ms})
    };
    serde_json::from_value(json!({
        "today": date_of(TODAY),
        "days_in_window": 1,
        "totals": {"tokens": 1_840_000, "calls": 1284, "cached_share": 0.61,
                   "model_ms": 11_520_000, "day_budget": day_budget},
        "days": days,
        "models": [
            model("Sonnet 5.5", 960_000, 690, 880_000, 563_000),
            model("Qwen3.8 Flash Next · local", 500_000, 246, 450_000, 247_500),
            model("Opus 5.5", 225_000, 142, 200_000, 132_000),
            model("Haiku 5.5", 155_000, 206, 150_000, 72_000),
        ],
        "agents": [
            agent("Coder", 702, 1_120_000, 1_000_000, 640_000, 7_080_000),
            agent("Autoresearcher", 188, 290_000, 260_000, 135_000, 2_280_000),
            agent("Reviewer", 214, 260_000, 230_000, 133_000, 1_320_000),
            agent("Grilling partner", 60, 50_000, 45_000, 20_000, 480_000),
            agent("No run", 120, 120_000, 100_000, 71_000, 360_000),
        ],
        "activities": [
            {"activity": "bash", "model_ms": 3_610_000, "calls": 214, "output_tokens": 61_000},
            {"activity": "read", "model_ms": 2_240_000, "calls": 402, "output_tokens": 38_000},
            {"activity": "edit", "model_ms": 1_870_000, "calls": 168, "output_tokens": 80_000},
            {"activity": "answering", "model_ms": 1_650_000, "calls": 231, "output_tokens": 52_000},
            {"activity": "grep", "model_ms": 1_020_000, "calls": 97, "output_tokens": 9_000},
            {"activity": "write", "model_ms": 560_000, "calls": 33, "output_tokens": 41_000},
            {"activity": "glob", "model_ms": 340_000, "calls": 52, "output_tokens": 4_000},
            {"activity": "web_fetch", "model_ms": 120_000, "calls": 11, "output_tokens": 2_000},
            {"activity": "todo", "model_ms": 70_000, "calls": 28, "output_tokens": 3_000},
            {"activity": "ask", "model_ms": 40_000, "calls": 1, "output_tokens": 500},
        ],
        "tool_waits": [
            {"tool": "ctest (radiance)", "calls": 31, "seconds": 2460},
            {"tool": "cmake --build", "calls": 22, "seconds": 1620},
            {"tool": "bench (gpu-bench)", "calls": 7, "seconds": 660},
            {"tool": "cargo test (moebius)", "calls": 12, "seconds": 300},
        ],
        "tool_output": {"results": 96, "bytes_before": 3_400_000, "bytes_after": 920_000},
    }))
    .unwrap()
}

fn usage_app(day_budget: Option<i64>) -> App {
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Usage));
    let _ = app.update(Message::Usage(usage::Message::Loaded(Ok(Box::new(
        usage_report(day_budget),
    )))));
    app
}

/// The usage screen drawn: compare `tests/snapshots/usage-tiny-skia.png` with `docs/design/usage.png`.
#[test]
fn the_usage_screen_shows_the_heatmap_models_and_agents_like_the_mockup() {
    draw_sized(
        &usage_app(Some(8_000_000)),
        "usage",
        &[
            "Usage",
            "Today",
            "7 days",
            "30 days",
            "Tokens",
            "1.84M",
            "23% of the 8M daily budget",
            "Model calls",
            "1,284",
            "1,433 tokens per call on average",
            "From cache",
            "61%",
            "Model time",
            "3 h 12 m",
            "Tokens per day",
            "Lighter means fewer tokens, darker means more",
            "Tokens by model",
            "1.84M today across 4 models",
            "Tokens per call",
            "Sonnet 5.5",
            "960k",
            "52%",
            "Where the model time goes",
            "bash",
            " · 214 calls",
            "Answering",
            "1 h 0 m",
            "Other",
            "Longest tool waits",
            "ctest (radiance)",
            "41 m",
            "Outputs cut from 3.4 MB to 920 KB",
            "By agent",
            "Autoresearcher",
            "Tokens today",
            "1.84M / 8M",
        ],
        Size::new(1440.0, 1500.0),
    );
}

/// The lens's checks of the usage: where their time went, drawn as `tests/snapshots/usage-lens-tiny-skia.png`.
#[test]
fn the_usage_screen_lists_where_the_time_of_the_lint_and_type_checks_goes() {
    let mut report = usage_report(Some(8_000_000));
    report.lens = serde_json::from_value(json!([
        {"phase": "deep", "tool": "cargo", "runs": 31, "skipped": 2, "ms": 412_000, "shown": 9},
        {"phase": "lint", "tool": "ruff", "runs": 140, "skipped": 0, "ms": 5_200, "shown": 17},
        {"phase": "syntax", "tool": "syntax", "runs": 96, "skipped": 0, "ms": 640, "shown": 1},
    ]))
    .unwrap();
    let mut app = App::with_snapshot(overview());
    let _ = app.update(Message::Select(Page::Usage));
    let _ = app.update(Message::Usage(usage::Message::Loaded(Ok(Box::new(report)))));
    draw_sized(
        &app,
        "usage-lens",
        &[
            "Lint and type checks",
            "Before the run ends",
            "After an edit",
            "Told to the agent",
            "6 m",
            "640 ms",
        ],
        Size::new(1440.0, 1900.0),
    );
    // A server that reports no lens time has no box.
    assert!(!shows(&usage_app(None), "Lint and type checks"));
}

#[test]
fn the_sidebar_meter_is_the_tokens_alone_without_a_budget_and_absent_before_the_usage_is_read() {
    let app = usage_app(None);
    assert!(shows(&app, "Tokens today"));
    assert!(shows(&app, "1.84M"));
    assert!(!shows(&app, "1.84M / 8M"));
    let unread = App::with_snapshot(overview());
    assert!(!shows(&unread, "Tokens today"));
}

#[test]
fn choosing_a_window_asks_for_it_and_an_answer_for_another_window_is_ignored() {
    let mut app = usage_app(Some(8_000_000));
    assert!(matches!(
        clicked(&app, "7 days").as_slice(),
        [Message::Usage(usage::Message::Choose(usage::Window::Week))]
    ));
    let _ = app.update(Message::Usage(usage::Message::Choose(usage::Window::Week)));
    // The answer to the window that was left does not replace what is shown.
    let _ = app.update(Message::Usage(usage::Message::Loaded(Ok(Box::new(
        usage_report(None),
    )))));
    assert!(shows(&app, "1.84M / 8M"));
    let mut week = usage_report(None);
    week.days_in_window = 7;
    week.totals.tokens = 7_000_000;
    let _ = app.update(Message::Usage(usage::Message::Loaded(Ok(Box::new(week)))));
    assert!(shows(&app, "7M"));
    assert!(shows(&app, "1M tokens a day on average"));
    assert!(shows(&app, "1.84M in the last 7 days across 4 models"));
}
