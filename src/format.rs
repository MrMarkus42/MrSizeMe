//! Numbers, sizes and dates as text in the current language
//! (German: 1.234.567 and 1,5 GB; English: 1,234,567 and 1.5 GB).

use std::io;

use crate::i18n::{Texts, tr};
use crate::tree::NO_TIME;

/// Size with binary units (1 KB = 1024 bytes), as the file managers show it.
pub fn size(bytes: u64) -> String {
    size_in(tr(), bytes)
}

pub fn size_in(t: &Texts, bytes: u64) -> String {
    const UNITS: [&str; 6] = ["KB", "MB", "GB", "TB", "PB", "EB"];
    if bytes < 1024 {
        return format!("{bytes} {}", t.bytes);
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let decimals = if value < 10.0 {
        2
    } else if value < 100.0 {
        1
    } else {
        0
    };
    let mut text = format!("{value:.decimals$}");
    // 1023.99 KB rounds up to "1024 KB"; show "1.00 MB" instead.
    if text == "1024" && unit < UNITS.len() - 1 {
        text = "1.00".into();
        unit += 1;
    }
    format!("{} {}", text.replace('.', &t.numbers.decimal.to_string()), UNITS[unit])
}

/// Whole number with group separators.
pub fn count(n: u64) -> String {
    count_in(tr(), n)
}

pub fn count_in(t: &Texts, n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(t.numbers.group);
        }
        out.push(ch);
    }
    out
}

/// Share of `part` in `whole` as "12,3 %".
pub fn percent(part: u64, whole: u64) -> String {
    percent_in(tr(), part, whole)
}

pub fn percent_in(t: &Texts, part: u64, whole: u64) -> String {
    format!("{:.1} %", share(part, whole) * 100.0).replace('.', &t.numbers.decimal.to_string())
}

/// Share of `part` in `whole` between 0 and 1.
pub fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 { 0.0 } else { (part as f64 / whole as f64).clamp(0.0, 1.0) }
}

/// Date (UTC) in the style of the current language; empty if unknown.
pub fn date(secs: i64) -> String {
    if tr().numbers.decimal == ',' { date_de(secs) } else { iso_date(secs) }
}

/// "24.11.2026"
pub fn date_de(secs: i64) -> String {
    if secs == NO_TIME {
        return String::new();
    }
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{d:02}.{m:02}.{y:04}")
}

/// "2026-11-24" (also for files that other programs read).
pub fn iso_date(secs: i64) -> String {
    if secs == NO_TIME {
        return String::new();
    }
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to (year, month, day); H. Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Short description of a file system error in the current language.
pub fn io_error(e: &io::Error) -> String {
    io_error_in(tr(), e)
}

pub fn io_error_in(t: &Texts, e: &io::Error) -> String {
    use io::ErrorKind as K;
    let text = match e.kind() {
        K::PermissionDenied => t.err_access_denied,
        K::NotFound => t.err_not_found,
        K::Interrupted | K::TimedOut => t.err_timeout,
        K::InvalidFilename => t.err_invalid_name,
        _ => return format!("{}: {e}", t.err_other),
    };
    match e.raw_os_error() {
        Some(code) => format!("{text} ({} {code})", t.err_system_code),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{DE, EN};

    #[test]
    fn sizes() {
        let s = |b| size_in(&DE, b);
        assert_eq!(s(0), "0 Bytes");
        assert_eq!(s(1023), "1023 Bytes");
        assert_eq!(s(1024), "1,00 KB");
        assert_eq!(s(1536), "1,50 KB");
        assert_eq!(s(15 * 1024 * 1024), "15,0 MB");
        assert_eq!(s(500 * 1024 * 1024), "500 MB");
        assert_eq!(s(1024 * 1024 - 1), "1,00 MB");
        assert_eq!(s(3 * 1024u64.pow(4)), "3,00 TB");
        assert_eq!(s(u64::MAX), "16,0 EB");
        assert_eq!(size_in(&EN, 1536), "1.50 KB");
        assert_eq!(size_in(&EN, 5), "5 bytes");
    }

    #[test]
    fn counts_and_percent() {
        assert_eq!(count_in(&DE, 0), "0");
        assert_eq!(count_in(&DE, 999), "999");
        assert_eq!(count_in(&DE, 1000), "1.000");
        assert_eq!(count_in(&DE, 1_234_567), "1.234.567");
        assert_eq!(count_in(&EN, 1_234_567), "1,234,567");
        assert_eq!(percent_in(&DE, 1, 3), "33,3 %");
        assert_eq!(percent_in(&EN, 1, 3), "33.3 %");
        assert_eq!(percent_in(&DE, 5, 0), "0,0 %");
    }

    #[test]
    fn dates() {
        assert_eq!(date_de(0), "01.01.1970");
        assert_eq!(date_de(1_795_910_400), "29.11.2026");
        assert_eq!(iso_date(951_782_400), "2000-02-29");
        assert_eq!(date_de(-86_400), "31.12.1969");
        assert_eq!(date_de(NO_TIME), "");
    }

    #[test]
    fn errors() {
        let e = io::Error::from_raw_os_error(if cfg!(windows) { 5 } else { 13 });
        assert!(io_error_in(&DE, &e).starts_with("Zugriff verweigert"));
        assert!(io_error_in(&EN, &e).starts_with("Access denied"));
    }
}
