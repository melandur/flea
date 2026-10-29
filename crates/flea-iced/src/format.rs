// Ports of ui/js/Format.js: GLib's SI one-decimal size and the one date form every surface prints.
use chrono::{Local, TimeZone};

const BYTES_PER_UNIT: f64 = 1000.0;
const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];

pub fn size(bytes: u64) -> String {
    if (bytes as f64) < BYTES_PER_UNIT {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= BYTES_PER_UNIT && unit < UNITS.len() - 1 {
        value /= BYTES_PER_UNIT;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

// "2026-09-12 15:29" in the machine's local wall clock.
pub fn date(mtime: i64) -> String {
    match Local.timestamp_opt(mtime, 0).single() {
        Some(t) => t.format("%Y-%m-%d %H:%M").to_string(),
        None => "--".into(),
    }
}

pub fn count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

const S_IFMT: u32 = 0o170000;
pub fn is_symlink(mode: u32) -> bool {
    mode & S_IFMT == 0o120000
}
pub fn is_executable(mode: u32) -> bool {
    mode & 0o111 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_glib_including_the_measured_round_down() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(999), "999 B");
        assert_eq!(size(1000), "1.0 kB");
        // AGENTS.md "Size formatting matches real GLib": 26.95 GB prints 26.9, measured.
        assert_eq!(size(26_950_000_000), "26.9 GB");
    }

    #[test]
    fn counts_group_by_thousands() {
        assert_eq!(count(7), "7");
        assert_eq!(count(100000), "100,000");
        assert_eq!(count(1234567), "1,234,567");
    }
}
