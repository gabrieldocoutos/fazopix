//! Parser for XP (Banco XP) credit card statements.
//!
//! Layout of the extracted text (labels and values often on separate lines):
//! - `Titular` followed by the holder name
//! - `Vencimento` followed by `DD/MM/YYYY`
//! - `Fatura fechada em` followed by `DD/MM/YYYY`
//! - `Data do Documento` followed by `DD/MM/YYYY Número do Documento` (boleto)
//! - card section header: `<NAME> - 4998********0808`, then the column header
//!   `Data Descrição R$ US$`, closed by `Subtotal X`
//!   - item: `DD/MM/YY <description>[ - Parcela K/N] <R$ amount>[ <US$ amount>]`
//!   - amounts have no currency symbol; credits/payments are printed negative
//! - `Despesas até a emissão desta fatura X` must match the sum of all items
//! - everything outside card sections is ignored.

use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;

use super::ParseError;
use super::common::{
    amount_on_line, check_total, date_after, invalid_date, normalize_lines, parse_amount, unknown,
    value_after,
};
use crate::statement::{Installment, Kind, Statement, Transaction};

static ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(\d{2}/\d{2}/\d{2}) (.+?)(?: - Parcela (\d+)/(\d+))? (-?[\d.]+,\d{2})(?: -?[\d.]+,\d{2})?$",
    )
    .unwrap()
});
static CARD_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^.+ - \d{4}\*+(\d{4})$").unwrap());

const COLUMN_HEADER: &str = "Data Descrição R$ US$";
const BILL_PAYMENT: &str = "Pagamentos Validos Normais";

pub fn parse(text: &str) -> Result<Statement, ParseError> {
    let lines = normalize_lines(text);

    let holder = value_after(&lines, "Titular")
        .map(str::to_string)
        .ok_or(ParseError::MissingField("titular"))?;
    let due_on = date_after(&lines, "Vencimento").ok_or(ParseError::MissingField("vencimento"))?;
    let closing_on = date_after(&lines, "Fatura fechada em")
        .ok_or(ParseError::MissingField("fatura fechada em"))?;
    let issued_on = date_after(&lines, "Data do Documento")
        .ok_or(ParseError::MissingField("data do documento"))?;
    let total = amount_on_line(&lines, "Despesas até a emissão desta fatura ")
        .ok_or(ParseError::MissingField("despesas até a emissão"))?;

    let mut transactions = Vec::new();
    let mut card: Option<String> = None;

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let line = line.as_str();

        if let Some(caps) = CARD_HEADER.captures(line) {
            card = Some(caps[1].to_string());
            continue;
        }
        let Some(last4) = &card else { continue };

        if line.is_empty() || line == COLUMN_HEADER {
            continue;
        }
        if line.strip_prefix("Subtotal ").and_then(parse_amount).is_some() {
            card = None;
            continue;
        }

        let caps = ITEM.captures(line).ok_or_else(|| unknown(line_no, line))?;
        // IOF lines put "R$" before the amount, which lands in the description.
        let description = caps[2].trim_end_matches(" R$").trim().to_string();
        // Paying the bill isn't spending; only track what was consumed.
        if description == BILL_PAYMENT {
            continue;
        }

        let date = NaiveDate::parse_from_str(&caps[1], "%d/%m/%y")
            .map_err(|_| invalid_date(line_no, line))?;
        let installment = match (caps.get(3), caps.get(4)) {
            (Some(k), Some(n)) => Some(Installment {
                current: k.as_str().parse().map_err(|_| unknown(line_no, line))?,
                total: n.as_str().parse().map_err(|_| unknown(line_no, line))?,
            }),
            _ => None,
        };
        let amount_cents = parse_amount(&caps[5]).ok_or_else(|| unknown(line_no, line))?;

        transactions.push(Transaction {
            date,
            description,
            amount_cents,
            // Credits (refunds, payments) are printed negative.
            kind: if amount_cents < 0 {
                Kind::Payment
            } else {
                Kind::Purchase
            },
            card: Some(last4.clone()),
            installment,
        });
    }

    check_total(total, &transactions)?;

    Ok(Statement {
        issuer: "XP",
        holder,
        issued_on,
        due_on,
        closing_on,
        transactions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
Olá, FULANO! Chegou sua fatura
Banco XP S.A. - CNPJ: 33.264.668/0001-03

Data do Documento
04/11/2026 Número do Documento

Titular
FULANO DE TAL
 Endereço
RUA X 1
 Vencimento

10/11/2026

Despesas até a emissão desta fatura 190,10

Fatura fechada em

03/11/2026
 Próximo fechamento

03/12/2026

TAL - 4998********0808

Data Descrição R$ US$

02/08/26 PG *NUVEM LURKMEI - Parcela 4/6 143,28 0,00

10/10/26 IOF Transacoes Exterior R$ 1,92

10/10/26 BATTLE.NET 54,90 11,03

12/10/26 ESTORNO LOJA -10,00 0,00

Subtotal 1.378,10

TAL - 4998********5830

Data Descrição R$ US$

05/10/26 Pagamentos Validos Normais -1.495,15

Subtotal 0,00

As informações deste demonstrativo são apresentadas de acordo com a regulamentação vigente.
";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn parses_header_fields() {
        let s = parse(SAMPLE).unwrap();
        assert_eq!(s.issuer, "XP");
        assert_eq!(s.holder, "FULANO DE TAL");
        assert_eq!(s.due_on, d(2026, 11, 10));
        assert_eq!(s.closing_on, d(2026, 11, 3));
        assert_eq!(s.issued_on, d(2026, 11, 4));
    }

    #[test]
    fn parses_items_and_skips_bill_payment() {
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
                (d(2026, 8, 2), "PG *NUVEM LURKMEI", 14328, Kind::Purchase, Some("0808"), Some((4, 6))),
                (d(2026, 10, 10), "IOF Transacoes Exterior", 192, Kind::Purchase, Some("0808"), None),
                (d(2026, 10, 10), "BATTLE.NET", 5490, Kind::Purchase, Some("0808"), None),
                (d(2026, 10, 12), "ESTORNO LOJA", -1000, Kind::Payment, Some("0808"), None),
            ]
        );
    }

    #[test]
    fn rejects_unknown_line_in_card_section() {
        let text = SAMPLE.replace("10/10/26 BATTLE.NET 54,90 11,03", "garbage");
        assert!(matches!(parse(&text), Err(ParseError::UnknownLine { .. })));
    }

    #[test]
    fn rejects_total_mismatch() {
        let text = SAMPLE.replace("fatura 190,10", "fatura 190,11");
        assert!(matches!(parse(&text), Err(ParseError::TotalMismatch { .. })));
    }
}
