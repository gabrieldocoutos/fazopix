//! Issuer detection and dispatch to issuer-specific statement parsers.

mod mercado_pago;
mod xp;

use std::fmt;

use crate::statement::Statement;

#[derive(Debug)]
pub enum ParseError {
    UnknownIssuer,
    MissingField(&'static str),
    UnknownLine { line_no: usize, text: String },
    InvalidDate { line_no: usize, text: String },
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
    } else {
        Err(ParseError::UnknownIssuer)
    }
}
