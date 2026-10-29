//! How the Usage page tells its figures: numbers of tokens in three figures, counts with separators, shares and times,
//! and the dates of the heatmap as day numbers with their weekday and month.

pub const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
pub const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A number of tokens in three figures: 960k, 1.84M, 148M.
pub fn short(n: i64) -> String {
    let (value, unit) = match n {
        ..1_000 => return n.to_string(),
        1_000..1_000_000 => (n as f64 / 1e3, "k"),
        _ => (n as f64 / 1e6, "M"),
    };
    let decimals = if value < 10.0 {
        2
    } else if value < 100.0 {
        1
    } else {
        0
    };
    let shown = format!("{value:.decimals$}");
    let shown = if shown.contains('.') {
        shown.trim_end_matches('0').trim_end_matches('.')
    } else {
        &shown
    };
    format!("{shown}{unit}")
}

/// A number of bytes in three figures: 640 B, 12.4 KB, 1.2 MB.
pub fn bytes(n: i64) -> String {
    match n {
        ..1_000 => format!("{n} B"),
        1_000..1_000_000 => format!("{} KB", short(n).trim_end_matches('k')),
        _ => format!("{} MB", short(n).trim_end_matches('M')),
    }
}

/// A whole number with thousands separators: 1,284.
pub fn grouped(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if n < 0 { format!("-{out}") } else { out }
}

pub fn percent(share: f64) -> String {
    format!("{:.0}%", share * 100.0)
}

/// A share of a whole as a percentage; 0 of nothing.
pub fn share(part: i64, whole: i64) -> String {
    percent(if whole > 0 {
        part as f64 / whole as f64
    } else {
        0.0
    })
}

/// A time as hours and minutes ("3 h 12 m"), minutes ("38 m") or seconds ("45 s").
pub fn duration(seconds: i64) -> String {
    match seconds {
        ..60 => format!("{seconds} s"),
        60..3600 => format!("{} m", seconds / 60),
        _ => format!("{} h {} m", seconds / 3600, seconds % 3600 / 60),
    }
}

// Dates: days since 1970-01-01 and the calendar, by Howard Hinnant's algorithms.

/// The day number of a `YYYY-MM-DD` date.
pub fn day_number(date: &str) -> Option<i64> {
    let mut parts = date.split('-').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

/// The month (0 to 11) and day of the month of a day number.
pub fn month_and_day(number: i64) -> (usize, i64) {
    let z = number + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 2
    } else {
        shifted - 10
    };
    (month as usize, day)
}

/// The day of the week of a day number, Sunday 0 (1970-01-01 was a Thursday).
pub fn weekday(number: i64) -> usize {
    (number + 4).rem_euclid(7) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_told_in_three_figures() {
        assert_eq!(bytes(640), "640 B");
        assert_eq!(bytes(12_400), "12.4 KB");
        assert_eq!(bytes(920_000), "920 KB");
        assert_eq!(bytes(3_400_000), "3.4 MB");
    }

    #[test]
    fn numbers_are_told_in_three_figures_with_separators_and_hours() {
        assert_eq!(short(950), "950");
        assert_eq!(short(960_000), "960k");
        assert_eq!(short(1_840_000), "1.84M");
        assert_eq!(short(8_000_000), "8M");
        assert_eq!(short(148_000_000), "148M");
        assert_eq!(grouped(1284), "1,284");
        assert_eq!(grouped(12), "12");
        assert_eq!(duration(45), "45 s");
        assert_eq!(duration(38 * 60), "38 m");
        assert_eq!(duration(3 * 3600 + 12 * 60), "3 h 12 m");
    }

    #[test]
    fn dates_are_day_numbers_with_their_weekday_and_month() {
        let day = day_number("2026-10-28").unwrap();
        assert_eq!(day, 20_754);
        assert_eq!(WEEKDAYS[weekday(day)], "Wed");
        assert_eq!(month_and_day(day), (9, 28));
        assert_eq!(month_and_day(day_number("2024-02-29").unwrap()), (1, 29));
        assert_eq!(day_number("nonsense"), None);
    }
}
