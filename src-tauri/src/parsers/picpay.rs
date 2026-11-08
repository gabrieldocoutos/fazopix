//! Parser for PicPay credit card statements.
//!
//! Layout of the extracted text (the fonts mangle some accents, e.g. `Transaçıes`,
//! so matching avoids accented words where it can):
//! - first line: `<HOLDER>,`; every page repeats a header with
//!   `DD/MM/YYYY | DD/MM/YYYY Vencimento: Fechamento:` (due date, closing date)
//! - boleto: `DD/MM/YYYY Data do documento` (issue date)
//! - card sections: `Picpay Card` (statement-level: payments, IOF) or
//!   `Picpay Card final NNNN`, closed by `Subtotal dos lançamentos X`
//!   - domestic item: `DD/MM <description>[PARCkk/nn] <amount>`
//!   - international item spans lines: `DD/MM <description>`, `Dólar: X`,
//!     `Câmbio do dia: R$ <rate> <US$> <R$>`
//! - `Total geral dos lançamentos X` must match the sum of all items

use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;

use super::ParseError;
use super::common::{
    amount_on_line, check_total, infer_date, invalid_date, normalize_lines, parse_amount, unknown,
};
use crate::statement::{Installment, Kind, Statement, Transaction};

static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d{2}/\d{2}/\d{4}").unwrap());
static CARD_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Picpay Card(?: final (\d{4}))?$").unwrap());
static ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{2})/(\d{2}) (.+?)(?: ?PARC(\d{2})/(\d{2}))? (-?[\d.]+,\d{2})$").unwrap()
});
/// First line of an international item; the amount comes on the exchange line.
static FOREIGN_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{2})/(\d{2}) (.+)$").unwrap());
static FOREIGN_AMOUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^C\S*mbio do dia: R\$ [\d.]+,\d{2} [\d.]+,\d{2} (-?[\d.]+,\d{2})$").unwrap()
});
static FOREIGN_DOLLAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^D\S*lar: [\d.]+,\d{2}$").unwrap());
static TABLE_TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Transa\S* (?:Nacionais|Internacionais)$").unwrap());
static PAGE_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^P\S*gina \d+/\d+$").unwrap());

const COLUMN_HEADER: &str = "Data Estabelecimento";
const SECTION_END: &str = "Subtotal dos lançamentos ";
const DETAILS_END: &str = "Total geral dos lançamentos ";
const BILL_PAYMENT: &str = "PAGAMENTO DE FATURA";

pub fn parse(text: &str) -> Result<Statement, ParseError> {
    let lines = normalize_lines(text);

    let first_line = lines
        .iter()
        .find(|l| !l.is_empty())
        .ok_or(ParseError::MissingField("holder"))?;
    let holder = first_line.trim_end_matches(',').to_string();
    let (due_on, closing_on) = lines
        .iter()
        .filter(|l| l.contains("Vencimento:") && l.contains("Fechamento:"))
        .find_map(|l| {
            let mut dates = DATE.find_iter(l).map(|m| parse_date(m.as_str()));
            Some((dates.next()??, dates.next()??))
        })
        .ok_or(ParseError::MissingField("vencimento / fechamento"))?;
    let issued_on = lines
        .iter()
        .find_map(|l| {
            let date = l.strip_suffix(" Data do documento")?;
            parse_date(date)
        })
        .ok_or(ParseError::MissingField("data do documento"))?;
    let total = amount_on_line(&lines, DETAILS_END)
        .ok_or(ParseError::MissingField("total geral dos lançamentos"))?;
    // Lines repeated at the top of every page.
    let page_header: Vec<&String> = lines.iter().filter(|l| !l.is_empty()).take(4).collect();

    let mut transactions = Vec::new();
    // `None` outside card sections; `Some(card)` inside one (`card` itself may be `None`).
    let mut section: Option<Option<String>> = None;
    // International item waiting for its amount line.
    let mut pending: Option<(usize, String)> = None;

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;

        if line.starts_with(DETAILS_END) {
            break;
        }
        if let Some(caps) = CARD_HEADER.captures(line) {
            section = Some(caps.get(1).map(|m| m.as_str().to_string()));
            continue;
        }
        let Some(card) = &section else { continue };
        let line = line.as_str();

        if line.is_empty()
            || page_header.iter().any(|h| *h == line)
            || PAGE_NUMBER.is_match(line)
            || TABLE_TITLE.is_match(line)
            || line.starts_with(COLUMN_HEADER)
        {
            continue;
        }

        if let Some((item_no, item_line)) = &pending {
            if FOREIGN_DOLLAR.is_match(line) {
                continue;
            }
            let amount_caps = FOREIGN_AMOUNT
                .captures(line)
                .ok_or_else(|| unknown(line_no, line))?;
            let caps = FOREIGN_ITEM.captures(item_line).unwrap();
            let tx = build(
                *item_no,
                item_line,
                &caps[1],
                &caps[2],
                &caps[3],
                None,
                &amount_caps[1],
                card.clone(),
                closing_on,
            )?;
            transactions.push(tx);
            pending = None;
            continue;
        }

        if line.starts_with(SECTION_END) {
            section = None;
            continue;
        }

        if let Some(caps) = ITEM.captures(line) {
            // Paying the bill isn't spending; only track what was consumed.
            if caps[3].trim() == BILL_PAYMENT {
                continue;
            }
            let installment = match (caps.get(4), caps.get(5)) {
                (Some(k), Some(n)) => Some(Installment {
                    current: k.as_str().parse().map_err(|_| unknown(line_no, line))?,
                    total: n.as_str().parse().map_err(|_| unknown(line_no, line))?,
                }),
                _ => None,
            };
            let tx = build(
                line_no,
                line,
                &caps[1],
                &caps[2],
                &caps[3],
                installment,
                &caps[6],
                card.clone(),
                closing_on,
            )?;
            transactions.push(tx);
        } else if FOREIGN_ITEM.is_match(line) {
            pending = Some((line_no, line.to_string()));
        } else {
            return Err(unknown(line_no, line));
        }
    }

    if let Some((line_no, line)) = pending {
        return Err(unknown(line_no, &line));
    }
    check_total(total, &transactions)?;

    Ok(Statement {
        issuer: "PicPay",
        holder,
        issued_on,
        due_on,
        closing_on,
        transactions,
    })
}

#[allow(clippy::too_many_arguments)]
fn build(
    line_no: usize,
    line: &str,
    day: &str,
    month: &str,
    description: &str,
    installment: Option<Installment>,
    amount: &str,
    card: Option<String>,
    closing_on: NaiveDate,
) -> Result<Transaction, ParseError> {
    let day: u32 = day.parse().map_err(|_| invalid_date(line_no, line))?;
    let month: u32 = month.parse().map_err(|_| invalid_date(line_no, line))?;
    // Printed dates are the original purchase dates, even for installments.
    let date = infer_date(day, month, installment.map_or(1, |i| i.current), closing_on)
        .ok_or_else(|| invalid_date(line_no, line))?;
    let amount_cents = parse_amount(amount).ok_or_else(|| unknown(line_no, line))?;
    Ok(Transaction {
        date,
        description: description.trim().to_string(),
        amount_cents,
        // Credits (refunds) are printed negative.
        kind: if amount_cents < 0 {
            Kind::Payment
        } else {
            Kind::Purchase
        },
        card,
        installment,
    })
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%d/%m/%Y").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
 FULANO DE TAL,
Rua X, 1, Bairro, 00000000 Cidade - RS
 10/11/2026 |   04/11/2026 Vencimento: Fechamento:
PicPay Mastercardﬁ BLACK
PicPay Bank Banco Mœltiplo S.A. CNPJ/CPF
05/11/2026 Data do documento
 FULANO DE TAL,
Rua X, 1, Bairro, 00000000 Cidade - RS
 10/11/2026 |   04/11/2026 Vencimento: Fechamento:
PicPay Mastercardﬁ BLACK
Picpay Card
Transaçıes Nacionais
Data Estabelecimento Valor (R$)
05/10 PAGAMENTO DE FATURA -5.253,00
11/10 IOF COMPRA INTERNACIONAL 0,91
Subtotal dos lançamentos 0,91
FULANO TAL
Picpay Card final 4024
Transaçıes Nacionais
Data Estabelecimento Valor (R$)
17/04 MARA REGINA BOPARC07/10 184,90
05/10 PADARIA ALECRIM LTDA 16,00
20/10 ESTORNO LOJA -6,00
Transaçıes Internacionais
Data Estabelecimento US$ R$
11/10 OBSIDIAN
Dólar: 5,00
Câmbio do dia: R$ 5,24 5,00 26,19
Subtotal dos lançamentos 221,09
 FULANO TAL
Picpay Card final 4032
Transaçıes Nacionais
Data Estabelecimento Valor (R$)
10/07 PELOTAS 33 PARC04/05 30,50
Subtotal dos lançamentos 30,50
Total geral dos lançamentos 252,50
Página 3/5
";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn parses_header_fields() {
        let s = parse(SAMPLE).unwrap();
        assert_eq!(s.issuer, "PicPay");
        assert_eq!(s.holder, "FULANO DE TAL");
        assert_eq!(s.due_on, d(2026, 11, 10));
        assert_eq!(s.closing_on, d(2026, 11, 4));
        assert_eq!(s.issued_on, d(2026, 11, 5));
    }

    #[test]
    fn parses_items_across_card_sections() {
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
                (d(2026, 10, 11), "IOF COMPRA INTERNACIONAL", 91, Kind::Purchase, None, None),
                (d(2026, 4, 17), "MARA REGINA BO", 18490, Kind::Purchase, Some("4024"), Some((7, 10))),
                (d(2026, 10, 5), "PADARIA ALECRIM LTDA", 1600, Kind::Purchase, Some("4024"), None),
                (d(2026, 10, 20), "ESTORNO LOJA", -600, Kind::Payment, Some("4024"), None),
                (d(2026, 10, 11), "OBSIDIAN", 2619, Kind::Purchase, Some("4024"), None),
                (d(2026, 7, 10), "PELOTAS 33", 3050, Kind::Purchase, Some("4032"), Some((4, 5))),
            ]
        );
    }

    #[test]
    fn rejects_unknown_line() {
        let text = SAMPLE.replace("05/10 PADARIA ALECRIM LTDA 16,00", "garbage");
        assert!(matches!(parse(&text), Err(ParseError::UnknownLine { .. })));
    }

    #[test]
    fn rejects_total_mismatch() {
        let text = SAMPLE.replace("lançamentos 252,50", "lançamentos 252,51");
        assert!(matches!(parse(&text), Err(ParseError::TotalMismatch { .. })));
    }
}
