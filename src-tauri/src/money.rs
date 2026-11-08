//! Brazilian Real amounts, stored as integer cents.

/// Parses `"R$ 1.234,56"` / `"-R$ 1.234,56"` into cents (`123456` / `-123456`).
pub fn parse_brl(s: &str) -> Option<i64> {
    let s = s.trim();
    let (negative, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, s),
    };
    let s = s.strip_prefix("R$")?.trim();
    let (int_part, dec_part) = s.split_once(',')?;
    if dec_part.len() != 2 || !dec_part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let int_digits: String = int_part.chars().filter(|&c| c != '.').collect();
    if int_digits.is_empty() || !int_digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let cents = int_digits.parse::<i64>().ok()? * 100 + dec_part.parse::<i64>().ok()?;
    Some(if negative { -cents } else { cents })
}

/// Formats cents as `"R$ 1.234,56"` (with a leading `-` for negatives).
pub fn format_brl(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    let int = (abs / 100).to_string();
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    format!("{sign}R$ {grouped},{:02}", abs % 100)
}
