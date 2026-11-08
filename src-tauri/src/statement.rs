//! Issuer-agnostic statement types produced by the parsers.

use std::fmt;

use chrono::NaiveDate;
use serde::Serialize;

use crate::money::format_brl;

#[derive(Debug, Serialize)]
pub struct Statement {
    pub issuer: &'static str,
    pub holder: String,
    pub issued_on: NaiveDate,
    pub due_on: NaiveDate,
    pub closing_on: NaiveDate,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Serialize)]
pub struct Transaction {
    /// Original purchase date (installments keep the date of the first purchase).
    pub date: NaiveDate,
    pub description: String,
    /// Positive = charge, negative = payment/credit.
    pub amount_cents: i64,
    pub kind: Kind,
    /// Last 4 digits of the card; `None` for statement-level movements (payments).
    pub card: Option<String>,
    pub installment: Option<Installment>,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Purchase,
    Payment,
}

#[derive(Debug, Serialize, Clone, Copy)]
pub struct Installment {
    pub current: u16,
    pub total: u16,
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} — {} | emitida {} | fechamento {} | vencimento {}",
            self.issuer, self.holder, self.issued_on, self.closing_on, self.due_on
        )?;
        for t in &self.transactions {
            let installment = t
                .installment
                .map(|i| format!(" [{}/{}]", i.current, i.total))
                .unwrap_or_default();
            writeln!(
                f,
                "{} | {:>4} | {:<36} | {:>14}{}",
                t.date,
                t.card.as_deref().unwrap_or("-"),
                t.description,
                format_brl(t.amount_cents),
                installment
            )?;
        }
        let installments = self.transactions.iter().filter(|t| t.installment.is_some()).count();
        write!(
            f,
            "{} transactions ({} installments)",
            self.transactions.len(),
            installments
        )
    }
}
