//! Minimal RFC 3339 parsing (no date crate in the workspace): seconds since the Unix epoch.

/// Days from 1970-01-01 to the civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.frac](Z|+HH:MM|-HH:MM)` into Unix seconds.
#[must_use]
pub fn parse_rfc3339(s: &str) -> Option<f64> {
    let s = s.trim();
    let (date, rest) = s.split_once(['T', 't', ' '])?;
    let mut d = date.split('-');
    let (y, m, day) = (
        d.next()?.parse::<i64>().ok()?,
        d.next()?.parse::<i64>().ok()?,
        d.next()?.parse::<i64>().ok()?,
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    let (clock, offset) = if let Some(c) = rest.strip_suffix(['Z', 'z']) {
        (c, 0.0)
    } else if let Some(i) = rest.rfind(['+', '-']) {
        let (c, o) = rest.split_at(i);
        let sign = if o.starts_with('-') { -1.0 } else { 1.0 };
        let (oh, om) = o[1..].split_once(':').unwrap_or((&o[1..], "0"));
        (
            c,
            sign * (oh.parse::<f64>().ok()? * 3600.0 + om.parse::<f64>().ok()? * 60.0),
        )
    } else {
        (rest, 0.0)
    };
    let mut t = clock.split(':');
    let h = t.next()?.parse::<f64>().ok()?;
    let mi = t.next()?.parse::<f64>().ok()?;
    let sec = t.next().unwrap_or("0").parse::<f64>().ok()?;
    let days = days_from_civil(y, m, day);
    Some(days as f64 * 86_400.0 + h * 3600.0 + mi * 60.0 + sec - offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utc_and_offsets() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(parse_rfc3339("2026-10-12T07:42:16Z"), Some(1_791_790_936.0));
        assert_eq!(
            parse_rfc3339("2026-10-12T09:42:16+02:00"),
            parse_rfc3339("2026-10-12T07:42:16Z")
        );
        assert_eq!(
            parse_rfc3339("2026-10-12T07:42:16.250Z"),
            Some(1_791_790_936.25)
        );
        assert_eq!(parse_rfc3339("nonsense"), None);
        assert_eq!(parse_rfc3339(""), None);
    }
}
