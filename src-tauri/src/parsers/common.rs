//! Helpers shared by the issuer-specific parsers.

use chrono::{Datelike, Months, NaiveDate};

use super::ParseError;
use crate::money::parse_brl;
use crate::statement::Transaction;

/// Splits extracted text into lines, collapsing runs of whitespace so matching
/// doesn't depend on PDF spacing, and mapping the Unicode minus (U+2212) to `-`.
pub fn normalize_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            l.replace('\u{2212}', "-")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Parses `"1.234,56"`, `"-1.234,56"`, `"R$ 1.234,56"` or `"-R$ 1.234,56"` into cents.
pub fn parse_amount(s: &str) -> Option<i64> {
    let s = s.trim();
    let (negative, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, s),
    };
    let s = s.strip_prefix("R$").unwrap_or(s).trim();
    let cents = parse_brl(&format!("R$ {s}"))?;
    Some(if negative { -cents } else { cents })
}

/// Month number for a Portuguese three-letter abbreviation (`"SET"` → 9).
pub fn pt_month(abbr: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "JAN", "FEV", "MAR", "ABR", "MAI", "JUN", "JUL", "AGO", "SET", "OUT", "NOV", "DEZ",
    ];
    let abbr = abbr.to_uppercase();
    MONTHS
        .iter()
        .position(|m| *m == abbr)
        .map(|i| i as u32 + 1)
}

/// Lines often carry only `DD/MM`. Installment K of a purchase is billed roughly K-1 months
/// after it was made, so pick the year whose date is closest to `closing - (K-1) months`
/// without being after the closing date.
pub fn infer_date(day: u32, month: u32, installment: u16, closing: NaiveDate) -> Option<NaiveDate> {
    let anchor =
        closing.checked_sub_months(Months::new(u32::from(installment.saturating_sub(1))))?;
    (closing.year() - 2..=closing.year())
        .filter_map(|y| NaiveDate::from_ymd_opt(y, month, day))
        .filter(|d| *d <= closing)
        .min_by_key(|d| (*d - anchor).num_days().abs())
}

/// First non-empty line after the first line equal to `label`.
pub fn value_after<'a>(lines: &'a [String], label: &str) -> Option<&'a str> {
    let start = lines.iter().position(|l| l == label)?;
    lines[start + 1..]
        .iter()
        .map(String::as_str)
        .find(|l| !l.is_empty())
}

/// `DD/MM/YYYY` at the start of the first non-empty line after `label`.
pub fn date_after(lines: &[String], label: &str) -> Option<NaiveDate> {
    let value = value_after(lines, label)?;
    NaiveDate::parse_from_str(value.get(..10)?, "%d/%m/%Y").ok()
}

/// First `DD/MM/YYYY` right after `prefix` on any line.
pub fn date_on_line(lines: &[String], prefix: &str) -> Option<NaiveDate> {
    lines.iter().find_map(|l| {
        let rest = l.strip_prefix(prefix)?.trim();
        NaiveDate::parse_from_str(rest.get(..10)?, "%d/%m/%Y").ok()
    })
}

/// Amount at the end of the first line starting with `prefix`.
pub fn amount_on_line(lines: &[String], prefix: &str) -> Option<i64> {
    lines.iter().find_map(|l| {
        l.strip_prefix(prefix)?;
        parse_amount(l.rsplit_once(' ')?.1)
    })
}

pub fn unknown(line_no: usize, text: &str) -> ParseError {
    ParseError::UnknownLine {
        line_no,
        text: text.to_string(),
    }
}

pub fn invalid_date(line_no: usize, text: &str) -> ParseError {
    ParseError::InvalidDate {
        line_no,
        text: text.to_string(),
    }
}

/// Fails unless the parsed transactions add up to the total printed on the statement.
pub fn check_total(expected_cents: i64, transactions: &[Transaction]) -> Result<(), ParseError> {
    let parsed_cents: i64 = transactions.iter().map(|t| t.amount_cents).sum();
    if parsed_cents == expected_cents {
        Ok(())
    } else {
        Err(ParseError::TotalMismatch {
            expected_cents,
            parsed_cents,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_amounts() {
        assert_eq!(parse_amount("1.234,56"), Some(123456));
        assert_eq!(parse_amount("-1.234,56"), Some(-123456));
        assert_eq!(parse_amount("R$ 54,00"), Some(5400));
        assert_eq!(parse_amount("-R$ 54,00"), Some(-5400));
        assert_eq!(parse_amount("abc"), None);
    }

    #[test]
    fn normalizes_unicode_minus_and_spaces() {
        assert_eq!(normalize_lines("  a \u{2212}R$  1,00 "), vec!["a -R$ 1,00"]);
    }

    #[test]
    fn maps_portuguese_months() {
        assert_eq!(pt_month("SET"), Some(9));
        assert_eq!(pt_month("dez"), Some(12));
        assert_eq!(pt_month("SEP"), None);
    }
}
