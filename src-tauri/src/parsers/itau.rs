//! Parser for Itaú credit card statements.
//!
//! Layout of the extracted text:
//! - `Titular <HOLDER>`, `Cartão NNNN.XXXX.XXXX.NNNN` (space may be missing)
//! - `Vencimento: DD/MM/YYYY`, `Emissão: DD/MM/YYYY` (no explicit closing date; the
//!   statement is issued on the closing day)
//! - `Pagamentos efetuados` … `Total dos pagamentos X`: bill payments (ignored)
//! - `Lançamentos: compras e saques`: cardholder name, column header, then items
//!   closed by `Lançamentos no cartão X`
//!   - item: `DD/MM <merchant><city> <amount>` followed by `<category> <city>`; the
//!     city is glued (sometimes truncated) to the merchant, so it is stripped
//! - `Total dos lançamentos atuais X` must match the sum of all items

use std::sync::LazyLock;

use regex::Regex;

use super::ParseError;
use super::common::{
    amount_on_line, check_total, date_on_line, infer_date, invalid_date, normalize_lines,
    parse_amount, unknown,
};
use crate::statement::{Kind, Statement, Transaction};

static CARD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Cartão ?[\dX]{4}\.X{4}\.X{4}\.(\d{4})$").unwrap());
static ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{2})/(\d{2}) (.+?) (-?[\d.]+,\d{2})$").unwrap());
static CARDHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-ZÀ-Ý ]+$").unwrap());

const PAYMENTS_START: &str = "Pagamentos efetuados";
const PAYMENTS_HEADER: &str = "DATA VALOR EM R$";
const PAYMENTS_END: &str = "Total dos pagamentos ";
const PURCHASES_START: &str = "Lançamentos: compras e saques";
const PURCHASES_HEADER: &str = "DATA ESTABELECIMENTO VALOR EM R$";
const PURCHASES_END: &str = "Lançamentos no cartão ";
const BILL_PAYMENT: &str = "PAGAMENTO";
/// Shortest city prefix accepted when stripping it from the merchant name.
const MIN_CITY_LEN: usize = 3;

enum Section {
    Outside,
    Payments,
    Purchases,
}

pub fn parse(text: &str) -> Result<Statement, ParseError> {
    let lines = normalize_lines(text);

    let holder = lines
        .iter()
        .find_map(|l| l.strip_prefix("Titular "))
        .map(str::to_string)
        .ok_or(ParseError::MissingField("titular"))?;
    let card = lines
        .iter()
        .find_map(|l| Some(CARD.captures(l)?[1].to_string()))
        .ok_or(ParseError::MissingField("cartão"))?;
    let due_on =
        date_on_line(&lines, "Vencimento:").ok_or(ParseError::MissingField("vencimento"))?;
    let issued_on =
        date_on_line(&lines, "Emissão:").ok_or(ParseError::MissingField("emissão"))?;
    let closing_on = issued_on;
    let total = amount_on_line(&lines, "Total dos lançamentos atuais ")
        .ok_or(ParseError::MissingField("total dos lançamentos atuais"))?;

    let mut transactions: Vec<Transaction> = Vec::new();
    let mut section = Section::Outside;
    // Whether the previous line was a purchase, so this one is its `<category> <city>`.
    let mut after_item = false;

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let line = line.as_str();

        if line == PAYMENTS_START {
            section = Section::Payments;
            continue;
        }
        if line == PURCHASES_START {
            section = Section::Purchases;
            continue;
        }
        if line.is_empty() {
            continue;
        }

        match section {
            Section::Outside => {}
            Section::Payments => {
                if line == PAYMENTS_HEADER {
                    continue;
                }
                if line.starts_with(PAYMENTS_END) {
                    section = Section::Outside;
                    continue;
                }
                let caps = ITEM.captures(line).ok_or_else(|| unknown(line_no, line))?;
                // Paying the bill isn't spending; only track what was consumed.
                if &caps[3] == BILL_PAYMENT {
                    continue;
                }
                transactions.push(build(line_no, line, &caps, &card, closing_on)?);
            }
            Section::Purchases => {
                if let Some(caps) = ITEM.captures(line) {
                    transactions.push(build(line_no, line, &caps, &card, closing_on)?);
                    after_item = true;
                    continue;
                }
                if after_item {
                    after_item = false;
                    let last = transactions.last_mut().unwrap();
                    last.description = strip_city(&last.description, line).to_string();
                    continue;
                }
                if line.starts_with(PURCHASES_END) {
                    section = Section::Outside;
                    continue;
                }
                if line == PURCHASES_HEADER || CARDHOLDER.is_match(line) {
                    continue;
                }
                return Err(unknown(line_no, line));
            }
        }
    }

    check_total(total, &transactions)?;

    Ok(Statement {
        issuer: "Itaú",
        holder,
        issued_on,
        due_on,
        closing_on,
        transactions,
    })
}

fn build(
    line_no: usize,
    line: &str,
    caps: &regex::Captures,
    card: &str,
    closing_on: chrono::NaiveDate,
) -> Result<Transaction, ParseError> {
    let day: u32 = caps[1].parse().unwrap();
    let month: u32 = caps[2].parse().unwrap();
    let date = infer_date(day, month, 1, closing_on).ok_or_else(|| invalid_date(line_no, line))?;
    let amount_cents = parse_amount(&caps[4]).ok_or_else(|| unknown(line_no, line))?;
    Ok(Transaction {
        date,
        description: caps[3].trim().to_string(),
        amount_cents,
        // Credits (refunds, payments) are printed negative.
        kind: if amount_cents < 0 {
            Kind::Payment
        } else {
            Kind::Purchase
        },
        card: Some(card.to_string()),
        installment: None,
    })
}

/// Removes the city glued to the end of `merchant`, using the `<category> <city>` line
/// that follows it. The glued city may be truncated (`"Sao Paul"` for `"Sao Paulo"`), so
/// the longest prefix of the city (at least [`MIN_CITY_LEN`] bytes) that ends `merchant`
/// is removed. The category may span several words, so every word split is tried.
fn strip_city<'a>(merchant: &'a str, category_city: &str) -> &'a str {
    let words: Vec<&str> = category_city.split(' ').collect();
    let mut best = 0;
    for start in 1..words.len() {
        let city = words[start..].join(" ");
        let found = (MIN_CITY_LEN..=city.len())
            .rev()
            .filter(|&n| city.is_char_boundary(n) && n < merchant.len())
            .find(|&n| merchant.ends_with(&city[..n]));
        best = best.max(found.unwrap_or(0));
    }
    merchant[..merchant.len() - best].trim_end()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    const SAMPLE: &str = "\
341-7Banco Itaá S.A.
Banco Itaú S.A.   341-7    34191752561751227252150484150003716260000026946
Titular FULANO DE TAL
Cartão4101.XXXX.XXXX.5894
Vencimento: 10/11/2026
Emissão: 03/11/2026
Previsão próx. Fechamento: 03/12/2026
 Pagamentos efetuados
DATA VALOR EM R$
05/10 PAGAMENTO -269,46
Total dos pagamentos -269,46
Lançamentos: compras e saques
FULANO TAL
DATA ESTABELECIMENTO VALOR EM R$
09/10 APPLE.COM/BILLSAO PAULO 19,90
outros  SAO PAULO
21/10 Pier SeguradoraSao Paul 245,66
outros  Sao Paulo
25/10 ESTORNO LOJA -5,00
outros  CURITIBA
Lançamentos no cartão 260,56
Total dos lançamentos atuais 260,56
Limites de crédito Valor em R$
";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn parses_header_fields() {
        let s = parse(SAMPLE).unwrap();
        assert_eq!(s.issuer, "Itaú");
        assert_eq!(s.holder, "FULANO DE TAL");
        assert_eq!(s.due_on, d(2026, 11, 10));
        assert_eq!(s.issued_on, d(2026, 11, 3));
        assert_eq!(s.closing_on, d(2026, 11, 3));
    }

    #[test]
    fn parses_items_and_strips_city() {
        let s = parse(SAMPLE).unwrap();
        let rows: Vec<_> = s
            .transactions
            .iter()
            .map(|t| (t.date, t.description.as_str(), t.amount_cents, t.kind, t.card.as_deref()))
            .collect();
        assert_eq!(
            rows,
            vec![
                (d(2026, 10, 9), "APPLE.COM/BILL", 1990, Kind::Purchase, Some("5894")),
                (d(2026, 10, 21), "Pier Seguradora", 24566, Kind::Purchase, Some("5894")),
                (d(2026, 10, 25), "ESTORNO LOJA", -500, Kind::Payment, Some("5894")),
            ]
        );
    }

    #[test]
    fn strips_city_with_multiword_category() {
        assert_eq!(strip_city("PADARIA XCURITIBA", "bares e restaurantes CURITIBA"), "PADARIA X");
        assert_eq!(strip_city("LOJA", "outros RIO"), "LOJA");
    }

    #[test]
    fn rejects_unknown_line() {
        let text = SAMPLE.replace("Lançamentos no cartão 260,56", "garbage");
        assert!(matches!(parse(&text), Err(ParseError::UnknownLine { .. })));
    }

    #[test]
    fn rejects_total_mismatch() {
        let text = SAMPLE.replace("atuais 260,56", "atuais 260,57");
        assert!(matches!(parse(&text), Err(ParseError::TotalMismatch { .. })));
    }
}
