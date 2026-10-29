//! The schedule of an automation: a cron expression of five fields (minute, hour, day of the month, month, day of the
//! week) in the server's local time, and the next minute it comes due.
//!
//! A field is `*`, a number, a range `a-b`, any of them with a step (`*/15`, `8-18/2`, `5/10` from 5 to the end), or a
//! list of these separated by commas. Months may be named `jan` to `dec` and days of the week `sun` to `sat`; Sunday is
//! 0 or 7. As in cron, when both the day of the month and the day of the week are restricted (neither starts with `*`),
//! a day that matches either comes due.

use time::{Date, Duration, Month, PrimitiveDateTime, Time};

/// The months as cron names them, January first.
const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// The days of the week as cron names them, Sunday first.
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/// How many years ahead the next firing is looked for; a schedule that never comes due (`0 0 30 2 *`) has none.
const YEARS_AHEAD: i32 = 5;

/// A parsed schedule: for each field, which of its values come due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    minutes: u64,
    hours: u32,
    days: u32,
    months: u16,
    weekdays: u8,
    /// Whether the day of the month and the day of the week were each restricted.
    days_restricted: bool,
    weekdays_restricted: bool,
}

/// One field of the expression: its name, its smallest and largest value and the names its values may have.
struct Field {
    name: &'static str,
    min: u32,
    max: u32,
    names: &'static [&'static str],
    /// The value a name stands for is its index plus this.
    first_name: u32,
}

const FIELDS: [Field; 5] = [
    Field {
        name: "minute",
        min: 0,
        max: 59,
        names: &[],
        first_name: 0,
    },
    Field {
        name: "hour",
        min: 0,
        max: 23,
        names: &[],
        first_name: 0,
    },
    Field {
        name: "day of the month",
        min: 1,
        max: 31,
        names: &[],
        first_name: 0,
    },
    Field {
        name: "month",
        min: 1,
        max: 12,
        names: &MONTHS,
        first_name: 1,
    },
    Field {
        name: "day of the week",
        min: 0,
        max: 7,
        names: &WEEKDAYS,
        first_name: 0,
    },
];

impl Field {
    fn value(&self, text: &str) -> Result<u32, String> {
        let lower = text.to_ascii_lowercase();
        if let Some(at) = self.names.iter().position(|name| *name == lower) {
            return Ok(at as u32 + self.first_name);
        }
        let value: u32 = text
            .parse()
            .map_err(|_| format!("the {} {text:?} is not a number", self.name))?;
        if value < self.min || value > self.max {
            return Err(format!(
                "the {} {value} is not between {} and {}",
                self.name, self.min, self.max
            ));
        }
        Ok(value)
    }

    /// The values of `text`, one bit each.
    fn parse(&self, text: &str) -> Result<u64, String> {
        let mut bits = 0u64;
        for item in text.split(',') {
            let (range, step) = match item.split_once('/') {
                Some((range, step)) => {
                    let step: u32 =
                        step.parse().ok().filter(|step| *step > 0).ok_or_else(|| {
                            format!(
                                "the step {step:?} of the {} is not a whole number above 0",
                                self.name
                            )
                        })?;
                    (range, Some(step))
                }
                None => (item, None),
            };
            let (from, to) = match range.split_once('-') {
                _ if range == "*" => (self.min, self.max),
                Some((from, to)) => (self.value(from)?, self.value(to)?),
                // `5/10` runs from 5 to the end, as cron has it.
                None if step.is_some() => (self.value(range)?, self.max),
                None => {
                    let value = self.value(range)?;
                    (value, value)
                }
            };
            if from > to {
                return Err(format!("the {} range {range} runs backwards", self.name));
            }
            let mut value = from;
            while value <= to {
                bits |= 1 << value;
                value += step.unwrap_or(1);
            }
        }
        Ok(bits)
    }
}

/// Parses a cron expression of five fields, or says what is wrong with it.
pub fn parse(text: &str) -> Result<Schedule, String> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let [minute, hour, day, month, weekday] = parts[..] else {
        return Err(format!(
            "a schedule has five fields (minute, hour, day of the month, month, day of the week), not {}",
            parts.len()
        ));
    };
    let weekdays = FIELDS[4].parse(weekday)?;
    Ok(Schedule {
        minutes: FIELDS[0].parse(minute)?,
        hours: FIELDS[1].parse(hour)? as u32,
        days: FIELDS[2].parse(day)? as u32,
        months: FIELDS[3].parse(month)? as u16,
        // Sunday is 0 and 7.
        weekdays: ((weekdays | (weekdays >> 7)) & 0x7f) as u8,
        days_restricted: !day.starts_with('*'),
        weekdays_restricted: !weekday.starts_with('*'),
    })
}

impl Schedule {
    /// Whether the day `date` comes due.
    fn day_matches(&self, date: Date) -> bool {
        let day = self.days & (1 << date.day()) != 0;
        let weekday = self.weekdays & (1 << date.weekday().number_days_from_sunday()) != 0;
        match (self.days_restricted, self.weekdays_restricted) {
            (true, true) => day || weekday,
            _ => day && weekday,
        }
    }

    /// The first minute after `after` at which the schedule comes due; none within [`YEARS_AHEAD`] years.
    pub fn next_after(&self, after: PrimitiveDateTime) -> Option<PrimitiveDateTime> {
        let start =
            after.replace_second(0).ok()?.replace_nanosecond(0).ok()? + Duration::minutes(1);
        let mut at = start;
        while at.year() <= start.year() + YEARS_AHEAD {
            let month = at.month() as u8;
            if self.months & (1 << month) == 0 {
                // The first minute of the next month.
                let (year, next) = match at.month() {
                    Month::December => (at.year() + 1, Month::January),
                    other => (at.year(), other.next()),
                };
                at = PrimitiveDateTime::new(
                    Date::from_calendar_date(year, next, 1).ok()?,
                    Time::MIDNIGHT,
                );
                continue;
            }
            if !self.day_matches(at.date()) {
                at = PrimitiveDateTime::new(at.date().next_day()?, Time::MIDNIGHT);
                continue;
            }
            if self.hours & (1 << at.hour()) == 0 {
                at = at.replace_minute(0).ok()? + Duration::hours(1);
                continue;
            }
            if self.minutes & (1 << at.minute()) == 0 {
                at += Duration::minutes(1);
                continue;
            }
            return Some(at);
        }
        None
    }
}

/// A local minute as `YYYY-MM-DD HH:MM`, as the store keeps and reads it.
pub fn format_minute(at: PrimitiveDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        at.year(),
        at.month() as u8,
        at.day(),
        at.hour(),
        at.minute()
    )
}

/// A local minute read from `YYYY-MM-DD HH:MM`.
pub fn parse_minute(text: &str) -> Option<PrimitiveDateTime> {
    let (date, time) = text.trim().split_once(' ')?;
    let mut date = date.split('-').map(str::parse::<i32>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (hour, minute) = time.split_once(':')?;
    let month = Month::try_from(u8::try_from(month).ok()?).ok()?;
    let date = Date::from_calendar_date(year, month, u8::try_from(day).ok()?).ok()?;
    let time = Time::from_hms(hour.parse().ok()?, minute.parse().ok()?, 0).ok()?;
    Some(PrimitiveDateTime::new(date, time))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> PrimitiveDateTime {
        parse_minute(text).unwrap()
    }

    fn next(schedule: &str, after: &str) -> String {
        format_minute(parse(schedule).unwrap().next_after(at(after)).unwrap())
    }

    #[test]
    fn the_next_firing_is_the_first_minute_that_matches_after_now() {
        assert_eq!(next("* * * * *", "2026-10-29 08:00"), "2026-10-29 08:01");
        assert_eq!(next("0 8 * * *", "2026-10-29 07:59"), "2026-10-29 08:00");
        // Not the minute it is now: that one is due already or past.
        assert_eq!(next("0 8 * * *", "2026-10-29 08:00"), "2026-10-30 08:00");
        assert_eq!(next("30 23 * * *", "2026-10-29 23:45"), "2026-10-30 23:30");
    }

    #[test]
    fn a_day_of_the_week_is_waited_for() {
        // 2026-10-29 is a Thursday; the next Friday at nine is the day after, the next Monday four days later.
        assert_eq!(next("0 9 * * fri", "2026-10-29 10:00"), "2026-10-30 09:00");
        assert_eq!(next("0 9 * * 5", "2026-10-30 09:00"), "2026-11-06 09:00");
        assert_eq!(next("0 8 * * 1-5", "2026-10-30 08:00"), "2026-11-02 08:00");
        // Sunday is 0 and 7.
        assert_eq!(next("0 0 * * 7", "2026-10-29 00:00"), "2026-11-01 00:00");
        assert_eq!(next("0 0 * * 0", "2026-10-29 00:00"), "2026-11-01 00:00");
    }

    #[test]
    fn steps_lists_and_ranges_pick_their_minutes() {
        assert_eq!(next("*/15 * * * *", "2026-10-29 08:01"), "2026-10-29 08:15");
        assert_eq!(next("*/15 * * * *", "2026-10-29 08:45"), "2026-10-29 09:00");
        assert_eq!(next("5/20 * * * *", "2026-10-29 08:30"), "2026-10-29 08:45");
        assert_eq!(
            next("0 8-18/4 * * *", "2026-10-29 12:00"),
            "2026-10-29 16:00"
        );
        assert_eq!(
            next("0 8-18/4 * * *", "2026-10-29 16:00"),
            "2026-10-30 08:00"
        );
        assert_eq!(
            next("10,40 6 * * *", "2026-10-29 06:10"),
            "2026-10-29 06:40"
        );
    }

    #[test]
    fn the_month_and_the_year_turn() {
        assert_eq!(next("0 0 1 * *", "2026-10-29 08:00"), "2026-11-01 00:00");
        assert_eq!(next("0 0 1 * *", "2026-12-15 08:00"), "2027-01-01 00:00");
        assert_eq!(next("59 23 31 * *", "2026-11-01 00:00"), "2026-12-31 23:59");
        assert_eq!(next("0 12 * feb *", "2026-10-29 08:00"), "2027-02-01 12:00");
        // The 29th of February comes in a leap year.
        assert_eq!(next("0 0 29 2 *", "2026-03-01 00:00"), "2028-02-29 00:00");
        assert_eq!(next("* * * * *", "2026-12-31 23:59"), "2027-01-01 00:00");
        assert!(
            parse("0 0 30 2 *")
                .unwrap()
                .next_after(at("2026-01-01 00:00"))
                .is_none()
        );
    }

    #[test]
    fn a_day_of_the_month_or_of_the_week_comes_due_when_both_are_restricted() {
        // The 1st, or any Monday: Monday 2026-11-02 comes before 2026-12-01.
        assert_eq!(next("0 9 1 * mon", "2026-11-01 10:00"), "2026-11-02 09:00");
        assert_eq!(next("0 9 1 * mon", "2026-11-30 10:00"), "2026-12-01 09:00");
        // With the day of the week `*`, only the day of the month counts.
        assert_eq!(next("0 9 1 * *", "2026-11-01 10:00"), "2026-12-01 09:00");
    }

    #[test]
    fn a_wrong_schedule_says_what_is_wrong() {
        let wrong = |text: &str| parse(text).unwrap_err();
        assert!(wrong("* * * *").contains("five fields"));
        assert!(wrong("60 * * * *").contains("minute 60"));
        assert!(wrong("* 24 * * *").contains("hour 24"));
        assert!(wrong("* * 0 * *").contains("day of the month 0"));
        assert!(wrong("* * * 13 *").contains("month 13"));
        assert!(wrong("* * * * 8").contains("day of the week 8"));
        assert!(wrong("*/0 * * * *").contains("step"));
        assert!(wrong("30-10 * * * *").contains("backwards"));
        assert!(wrong("x * * * *").contains("not a number"));
    }

    #[test]
    fn a_minute_reads_back_as_it_was_written() {
        assert_eq!(format_minute(at("2026-01-05 07:03")), "2026-01-05 07:03");
        assert!(parse_minute("2026-13-05 07:03").is_none());
        assert!(parse_minute("yesterday").is_none());
    }
}
