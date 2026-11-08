//! Parser for Nubank credit card statements.
//!
//! Layout of the extracted text (every page after the first repeats a header):
//! - page header: holder name, then `FATURA DD MMM YYYY EMISSÃO E ENVIO DD MMM YYYY`
//!   (due date and issue date; months are Portuguese abbreviations), and `N de M`
//! - `Período vigente DD MMM a DD MMM`: the billing period; its end is the closing date
//! - `Total de compras de todos os cartões, … R$ X` must match the sum of all items
//! - `TRANSAÇÕES DE …` starts the line items, `Em cumprimento à regulação` ends them:
//!   - group headers: `<cardholder> R$ X` and `Pagamentos -R$ X`
//!   - item: `DD MMM [•••• NNNN ]<description>[ - Parcela K/N] [-]R$ X`
//!   - dates have no year and installments carry the date they were billed in this
//!     period, so the original purchase date is K-1 months earlier

use std::sync::LazyLock;

use chrono::{Months, NaiveDate};
use regex::Regex;

use super::ParseError;
use super::common::{
    amount_on_line, check_total, infer_date, invalid_date, normalize_lines, parse_amount,
    pt_month, unknown,
};
use crate::statement::{Installment, Kind, Statement, Transaction};

static PAGE_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^FATURA (\d{2} [A-Z]{3} \d{4}) EMISSÃO E ENVIO (\d{2} [A-Z]{3} \d{4})$").unwrap()
});
static PERIOD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^Período vigente:? \d{2} [A-Z]{3} a (\d{2}) ([A-Z]{3})$").unwrap()
});
static ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\d{2}) ([A-Z]{3}) (?:•+ (\d{4}) )?(.+?)(?: - Parcela (\d+)/(\d+))? (-?R\$ [\d.]+,\d{2})$",
    )
    .unwrap()
});
static GROUP_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^.+ -?R\$ [\d.]+,\d{2}$").unwrap());
static PAGE_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+ de \d+$").unwrap());

const DETAILS_START: &str = "TRANSAÇÕES DE ";
const DETAILS_END: &str = "Em cumprimento à regulação";
const BILL_PAYMENT: &str = "Pagamento em ";

pub fn parse(text: &str) -> Result<Statement, ParseError> {
    let lines = normalize_lines(text);

    let header_idx = lines
        .iter()
        .position(|l| PAGE_HEADER.is_match(l))
        .ok_or(ParseError::MissingField("fatura / emissão"))?;
    let header = PAGE_HEADER.captures(&lines[header_idx]).unwrap();
    let due_on = parse_long_date(&header[1]).ok_or(ParseError::MissingField("fatura"))?;
    let issued_on =
        parse_long_date(&header[2]).ok_or(ParseError::MissingField("emissão e envio"))?;
    let holder = lines[..header_idx]
        .iter()
        .rev()
        .find(|l| !l.is_empty())
        .cloned()
        .ok_or(ParseError::MissingField("holder"))?;
    let closing_on = lines
        .iter()
        .find_map(|l| {
            let caps = PERIOD.captures(l)?;
            infer_date(caps[1].parse().ok()?, pt_month(&caps[2])?, 1, issued_on)
        })
        .ok_or(ParseError::MissingField("período vigente"))?;
    let total = amount_on_line(&lines, "Total de compras de todos os cartões")
        .ok_or(ParseError::MissingField("total de compras"))?;

    let start = lines
        .iter()
        .position(|l| l.starts_with(DETAILS_START))
        .ok_or(ParseError::MissingField("transações"))?;

    let mut transactions = Vec::new();
    for (idx, line) in lines.iter().enumerate().skip(start + 1) {
        let line_no = idx + 1;
        let line = line.as_str();

        if line.starts_with(DETAILS_END) {
            break;
        }
        if line.is_empty()
            || *line == holder
            || PAGE_HEADER.is_match(line)
            || PAGE_NUMBER.is_match(line)
        {
            continue;
        }

        let Some(caps) = ITEM.captures(line) else {
            // Cardholder / payments group headers carry only the group total.
            if GROUP_HEADER.is_match(line) {
                continue;
            }
            return Err(unknown(line_no, line));
        };
        let description = caps[4].trim().to_string();
        // Paying the bill isn't spending; only track what was consumed.
        if description.starts_with(BILL_PAYMENT) {
            continue;
        }

        let installment = match (caps.get(5), caps.get(6)) {
            (Some(k), Some(n)) => Some(Installment {
                current: k.as_str().parse().map_err(|_| unknown(line_no, line))?,
                total: n.as_str().parse().map_err(|_| unknown(line_no, line))?,
            }),
            _ => None,
        };
        let day: u32 = caps[1].parse().unwrap();
        let month = pt_month(&caps[2]).ok_or_else(|| invalid_date(line_no, line))?;
        let billed_on =
            infer_date(day, month, 1, closing_on).ok_or_else(|| invalid_date(line_no, line))?;
        let months_back = installment.map_or(0, |i| u32::from(i.current.saturating_sub(1)));
        let date = billed_on
            .checked_sub_months(Months::new(months_back))
            .ok_or_else(|| invalid_date(line_no, line))?;
        let amount_cents = parse_amount(&caps[7]).ok_or_else(|| unknown(line_no, line))?;

        transactions.push(Transaction {
            date,
            description,
            amount_cents,
            // Credits (refunds) are printed negative.
            kind: if amount_cents < 0 {
                Kind::Payment
            } else {
                Kind::Purchase
            },
            card: caps.get(3).map(|m| m.as_str().to_string()),
            installment,
        });
    }

    check_total(total, &transactions)?;

    Ok(Statement {
        issuer: "Nubank",
        holder,
        issued_on,
        due_on,
        closing_on,
        transactions,
    })
}

/// Parses `"09 NOV 2026"`.
fn parse_long_date(s: &str) -> Option<NaiveDate> {
    let mut parts = s.split(' ');
    let day = parts.next()?.parse().ok()?;
    let month = pt_month(parts.next()?)?;
    let year = parts.next()?.parse().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Olá, FULANO.
Data de vencimento  09 NOV 2026
Período vigente  30 SET a 31 OUT
 1  de 5

FULANO DE TAL
FATURA 09 NOV 2026 EMISSÃO E ENVIO 31 OUT 2026
Nu Pagamentos S.A.
RESUMO DA FATURA ATUAL
Pagamento recebido \u{2212}R$ 54,00
Total de compras de todos os cartões, 30 SET a 31 OUT R$ 74,00
 4  de 5

FULANO DE TAL
FATURA 09 NOV 2026 EMISSÃO E ENVIO 31 OUT 2026
TRANSAÇÕES DE 30 SET A 31 OUT
Fulano D Tal R$ 74,00
30 SET •••• 5255 Loja Comercio de - Parcela 10/10 R$ 54,00
02 OUT •••• 5255 Padaria R$ 30,00
03 OUT •••• 5255 Estorno Padaria -R$ 10,00
Pagamentos -R$ 54,00
05 OUT Pagamento em 05 OUT \u{2212}R$ 54,00
Em cumprimento à regulação do Banco Central
 5  de 5
";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn parses_header_fields() {
        let s = parse(SAMPLE).unwrap();
        assert_eq!(s.issuer, "Nubank");
        assert_eq!(s.holder, "FULANO DE TAL");
        assert_eq!(s.due_on, d(2026, 11, 9));
        assert_eq!(s.issued_on, d(2026, 10, 31));
        assert_eq!(s.closing_on, d(2026, 10, 31));
    }

    #[test]
    fn parses_items_and_back_computes_installment_dates() {
        let s = parse(SAMPLE).unwrap();
        let rows: Vec<_> = s
            .transactions
            .iter()
            .map(|t| {
                (
                    t.date,
                    t.description.as_str(),
                    t.amount_cents,
                    t.kind,
                    t.card.as_deref(),
                    t.installment.map(|i| (i.current, i.total)),
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                (d(2025, 12, 30), "Loja Comercio de", 5400, Kind::Purchase, Some("5255"), Some((10, 10))),
                (d(2026, 10, 2), "Padaria", 3000, Kind::Purchase, Some("5255"), None),
                (d(2026, 10, 3), "Estorno Padaria", -1000, Kind::Payment, Some("5255"), None),
            ]
        );
    }

    #[test]
    fn rejects_unknown_line() {
        let text = SAMPLE.replace("02 OUT •••• 5255 Padaria R$ 30,00", "garbage");
        assert!(matches!(parse(&text), Err(ParseError::UnknownLine { .. })));
    }

    #[test]
    fn rejects_total_mismatch() {
        let text = SAMPLE.replace("31 OUT R$ 74,00", "31 OUT R$ 75,00");
        assert!(matches!(parse(&text), Err(ParseError::TotalMismatch { .. })));
    }
}
