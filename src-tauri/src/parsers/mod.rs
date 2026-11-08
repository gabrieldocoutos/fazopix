//! Issuer detection and dispatch to issuer-specific statement parsers.

mod common;
mod itau;
mod mercado_pago;
mod nubank;
mod picpay;
mod xp;

use std::fmt;

use crate::money::format_brl;
use crate::statement::Statement;

#[derive(Debug)]
pub enum ParseError {
    UnknownIssuer,
    MissingField(&'static str),
    UnknownLine { line_no: usize, text: String },
    InvalidDate { line_no: usize, text: String },
    /// Parsed transactions don't add up to the total printed on the statement.
    TotalMismatch { expected_cents: i64, parsed_cents: i64 },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownIssuer => write!(f, "unsupported statement issuer"),
            Self::MissingField(field) => write!(f, "statement field not found: {field}"),
            Self::UnknownLine { line_no, text } => {
                write!(f, "unrecognized line {line_no}: {text:?}")
            }
            Self::InvalidDate { line_no, text } => {
                write!(f, "could not resolve date on line {line_no}: {text:?}")
            }
            Self::TotalMismatch {
                expected_cents,
                parsed_cents,
            } => write!(
                f,
                "transactions add up to {} but the statement total is {}",
                format_brl(*parsed_cents),
                format_brl(*expected_cents)
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses extracted statement text, picking the parser by issuer.
pub fn parse(text: &str) -> Result<Statement, ParseError> {
    println!("{}", text);
    if text.contains("Mercado Pago") {
        mercado_pago::parse(text)
    } else if text.contains("Banco XP S.A.") {
        xp::parse(text)
    } else if text.contains("Nu Pagamentos S.A.") {
        nubank::parse(text)
    } else if text.contains("PicPay Bank Banco") {
        // Accents in this PDF's fonts come out mangled ("Múltiplo" → "Mœltiplo").
        picpay::parse(text)
    } else if text.contains("Banco Itaú S.A.") {
        itau::parse(text)
    } else {
        Err(ParseError::UnknownIssuer)
    }
}
