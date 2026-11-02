//! Parsing Docker CLI output. Docker prints human sizes ("173.1MB") and
//! local timestamps ("2026-11-02 16:26:04 +0530 IST"), so both are parsed here
//! with tests against real output shapes.

use serde::Deserialize;

/// Parse a Docker size string such as "1.205GB", "4.1kB", "0B",
/// "98.3MB (4%)" or "1.5GiB". Returns `None` for "N/A" or empty.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.split_whitespace().next()?.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("n/a") {
        return None;
    }
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let value: f64 = num.parse().ok()?;
    let mult: f64 = match unit {
        "" | "B" => 1.0,
        "kB" | "KB" | "k" => 1e3,
        "MB" | "M" => 1e6,
        "GB" | "G" => 1e9,
        "TB" | "T" => 1e12,
        "PB" => 1e15,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((value * mult).round() as u64)
}

/// Parse "YYYY-MM-DD HH:MM:SS +HHMM ..." into Unix milliseconds.
pub fn parse_timestamp(s: &str) -> Option<i64> {
    let mut parts = s.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let offset = parts.next().unwrap_or("+0000");

    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time
        .split(':')
        .map(|p| p.split('.').next().unwrap_or("0").parse::<i64>());
    let (hh, mm, ss) = (t.next()?.ok()?, t.next()?.ok()?, t.next()?.ok()?);

    let (sign, digits) = match offset.as_bytes().first()? {
        b'+' => (1, &offset[1..]),
        b'-' => (-1, &offset[1..]),
        _ => (1, "0000"),
    };
    if digits.len() != 4 {
        return None;
    }
    let off_min = digits[..2].parse::<i64>().ok()? * 60 + digits[2..].parse::<i64>().ok()?;

    let days = days_from_civil(y, m, day);
    let secs = days * 86_400 + hh * 3600 + mm * 60 + ss - sign * off_min * 60;
    Some(secs * 1000)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (H. Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Look up one label in Docker's "k=v,k2=v2" label string.
pub fn label<'a>(labels: &'a str, key: &str) -> Option<&'a str> {
    labels.split(',').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then_some(v)
    })
}

pub fn parse_count(s: &str) -> Option<u64> {
    s.trim().parse().ok()
}

// ---- Raw shapes of `docker system df -v --format '{{json .}}'` -----------

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct RawDf {
    pub images: Option<Vec<RawImage>>,
    pub containers: Option<Vec<RawContainer>>,
    pub volumes: Option<Vec<RawVolume>>,
    pub build_cache: Option<Vec<RawBuildCache>>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct RawImage {
    pub containers: String,
    pub created_at: String,
    #[serde(rename = "ID")]
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub shared_size: String,
    pub size: String,
    pub unique_size: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct RawContainer {
    pub created_at: String,
    #[serde(rename = "ID")]
    pub id: String,
    pub image: String,
    pub labels: String,
    pub mounts: String,
    pub names: String,
    pub size: String,
    pub state: String,
    pub status: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct RawVolume {
    pub driver: String,
    pub labels: String,
    pub links: String,
    pub name: String,
    pub size: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct RawBuildCache {
    #[serde(rename = "ID")]
    pub id: String,
    pub cache_type: String,
    pub description: String,
    pub size: String,
    pub created_since: String,
    pub last_used_since: String,
    pub usage_count: serde_json::Value,
    pub in_use: serde_json::Value,
    pub shared: serde_json::Value,
}

/// `docker system df --format '{{json .}}'` prints one object per line.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct RawDfSummaryLine {
    #[serde(rename = "Type")]
    pub kind: String,
    pub total_count: String,
    pub active: String,
    pub size: String,
    pub reclaimable: String,
}

/// Docker prints booleans in templates as "true"/"false" strings or bools.
pub fn truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_sizes() {
        assert_eq!(parse_size("0B"), Some(0));
        assert_eq!(parse_size("4.1kB"), Some(4_100));
        assert_eq!(parse_size("173.1MB"), Some(173_100_000));
        assert_eq!(parse_size("1.205GB"), Some(1_205_000_000));
        assert_eq!(parse_size("98.3MB (4%)"), Some(98_300_000));
        assert_eq!(parse_size("1GiB"), Some(1_073_741_824));
        assert_eq!(parse_size("N/A"), None);
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("12XB"), None);
    }

    #[test]
    fn parses_docker_timestamps_with_offsets() {
        // 2026-11-02 16:26:04 +0530 == 2026-11-02 10:56:04 UTC
        let ms = parse_timestamp("2026-11-02 16:26:04 +0530 IST").unwrap();
        let utc = parse_timestamp("2026-11-02 10:56:04 +0000 UTC").unwrap();
        assert_eq!(ms, utc);
        assert_eq!(parse_timestamp("1970-01-01 00:00:00 +0000 UTC"), Some(0));
        assert_eq!(parse_timestamp("garbage"), None);
    }

    #[test]
    fn reads_labels() {
        let l = "com.docker.compose.project=shop,com.docker.compose.service=web";
        assert_eq!(label(l, "com.docker.compose.project"), Some("shop"));
        assert_eq!(label(l, "missing"), None);
    }
}
