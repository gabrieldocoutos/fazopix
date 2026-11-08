//! Parser for Mercado Pago credit card statements ("fatura").
//!
//! Layout of the extracted text (one item per line, page headers repeat):
//! - first line: holder name; every page repeats it followed by `Vencimento: DD/MM/YYYY`
//! - `Emitida em: DD/MM/YYYY`, `Fechamento da fatura DD/MM/YYYY`
//! - `Detalhes de consumo` starts the line items:
//!   - `Movimentações na fatura`: payments/credits, printed positive
//!   - `Cartão Visa [************NNNN]`: card section (header repeats per page),
//!     closed by `Total R$ X`, which must match the sum of the card's items
//!   - item: `DD/MM <description>[ Parcela K de N] R$ 1.234,56`
//! - `Seu cartão de crédito` starts informational sections (ignored).

use std::sync::LazyLock;

use regex::Regex;

use super::ParseError;
use super::common::{check_total, date_on_line, infer_date, normalize_lines, unknown};
use crate::money::parse_brl;
use crate::statement::{Installment, Kind, Statement, Transaction};

static ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{2})/(\d{2}) (.+?)(?: Parcela (\d+) de (\d+))? (-? ?R\$ [\d.]+,\d{2})$")
        .unwrap()
});
static CARD_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Cartão .*\[\**(\d{4})\]$").unwrap());

const DETAILS_START: &str = "Detalhes de consumo";
const PAYMENTS_START: &str = "Movimentações na fatura";
const DETAILS_END: &str = "Seu cartão de crédito";
const COLUMN_HEADER: &str = "Data Movimentações Valor em R$";
const BILL_PAYMENT: &str = "Pagamento da fatura";

enum Section {
    Preamble,
    Details,
    Payments,
    Card(String),
    AfterCard,
}

pub fn parse(text: &str) -> Result<Statement, ParseError> {
    let lines = normalize_lines(text);

    let holder = lines
        .iter()
        .find(|l| !l.is_empty())
        .cloned()
        .ok_or(ParseError::MissingField("holder"))?;
    let issued_on =
        date_on_line(&lines, "Emitida em:").ok_or(ParseError::MissingField("emitida em"))?;
    let due_on =
        date_on_line(&lines, "Vencimento:").ok_or(ParseError::MissingField("vencimento"))?;
    let closing_on = date_on_line(&lines, "Fechamento da fatura")
        .ok_or(ParseError::MissingField("fechamento da fatura"))?;

    let mut transactions = Vec::new();
    let mut section = Section::Preamble;
    // Index of the current card section's first transaction (headers repeat per page).
    let mut card_start = 0;

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let line = line.as_str();

        if let Section::Preamble = section {
            if line == DETAILS_START {
                section = Section::Details;
            }
            continue;
        }

        if line.is_empty()
            || line == holder
            || line == COLUMN_HEADER
            || line.starts_with("Vencimento:")
        {
            continue;
        }
        if line == DETAILS_END {
            break;
        }
        if line == PAYMENTS_START {
            section = Section::Payments;
            continue;
        }
        if let Some(caps) = CARD_HEADER.captures(line) {
            if !matches!(&section, Section::Card(last4) if *last4 == caps[1]) {
                card_start = transactions.len();
            }
            section = Section::Card(caps[1].to_string());
            continue;
        }
        if let Section::Card(_) = section
            && let Some(total) = line.strip_prefix("Total ").and_then(parse_brl)
        {
            check_total(total, &transactions[card_start..])?;
            section = Section::AfterCard;
            continue;
        }

        let (kind, card) = match &section {
            Section::Payments => (Kind::Payment, None),
            Section::Card(last4) => (Kind::Purchase, Some(last4.clone())),
            _ => return Err(unknown(line_no, line)),
        };
        let caps = ITEM.captures(line).ok_or_else(|| unknown(line_no, line))?;
        // Paying the bill isn't spending; only track what was consumed.
        if caps[3].contains(BILL_PAYMENT) {
            continue;
        }

        let day: u32 = caps[1].parse().unwrap();
        let month: u32 = caps[2].parse().unwrap();
        let installment = match (caps.get(4), caps.get(5)) {
            (Some(k), Some(n)) => Some(Installment {
                current: k.as_str().parse().map_err(|_| unknown(line_no, line))?,
                total: n.as_str().parse().map_err(|_| unknown(line_no, line))?,
            }),
            _ => None,
        };
        let date = infer_date(day, month, installment.map_or(1, |i| i.current), closing_on)
            .ok_or_else(|| ParseError::InvalidDate {
                line_no,
                text: line.to_string(),
            })?;
        let amount = parse_brl(&caps[6]).ok_or_else(|| unknown(line_no, line))?;

        transactions.push(Transaction {
            date,
            description: caps[3].trim().to_string(),
            // Payments/credits are printed positive but reduce the balance.
            amount_cents: if kind == Kind::Payment {
                -amount
            } else {
                amount
            },
            kind,
            card,
            installment,
        });
    }

    Ok(Statement {
        issuer: "Mercado Pago",
        holder,
        issued_on,
        due_on,
        closing_on,
        transactions,
    })
}

