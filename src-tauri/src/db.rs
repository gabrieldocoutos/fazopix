//! SQLite persistence for parsed statements.

use std::path::Path;
use std::sync::Mutex;

use chrono::NaiveDate;
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::statement::{Installment, Statement};

/// A transaction as shown on the "Gastos cartão de crédito" screen.
#[derive(Debug, Serialize)]
pub struct MonthTransaction {
    pub id: i64,
    pub date: NaiveDate,
    pub description: String,
    pub amount_cents: i64,
    pub card: Option<String>,
    pub installment: Option<Installment>,
    pub issuer: String,
}

/// A monthly income entry ("Entrada"), net of taxes.
#[derive(Debug, Serialize)]
pub struct Income {
    pub id: i64,
    pub year: i32,
    pub month: u32,
    pub description: String,
    pub amount_cents: i64,
}

/// Schema migrations, applied in order. `PRAGMA user_version` holds how many ran.
const MIGRATIONS: &[&str] = &[r"
CREATE TABLE statements (
    id          INTEGER PRIMARY KEY,
    issuer      TEXT NOT NULL,
    holder      TEXT NOT NULL,
    issued_on   TEXT NOT NULL,
    due_on      TEXT NOT NULL,
    closing_on  TEXT NOT NULL,
    file_name   TEXT NOT NULL,
    imported_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (issuer, holder, closing_on)
);
CREATE TABLE transactions (
    id                  INTEGER PRIMARY KEY,
    statement_id        INTEGER NOT NULL REFERENCES statements(id) ON DELETE CASCADE,
    position            INTEGER NOT NULL,
    date                TEXT NOT NULL,
    description         TEXT NOT NULL,
    amount_cents        INTEGER NOT NULL,
    kind                TEXT NOT NULL CHECK (kind IN ('purchase', 'payment')),
    card                TEXT,
    installment_current INTEGER,
    installment_total   INTEGER
);
CREATE INDEX transactions_statement_id ON transactions(statement_id);
CREATE INDEX transactions_date ON transactions(date);
", r"
CREATE TABLE incomes (
    id           INTEGER PRIMARY KEY,
    year         INTEGER NOT NULL,
    month        INTEGER NOT NULL CHECK (month BETWEEN 1 AND 12),
    description  TEXT NOT NULL,
    amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX incomes_year_month ON incomes(year, month);
"];

pub struct Db(Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    fn init(mut conn: Connection) -> rusqlite::Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&mut conn)?;
        Ok(Self(Mutex::new(conn)))
    }

    /// Stores the statement and its transactions, replacing any previous import
    /// of the same statement (same issuer, holder and closing date).
    pub fn save_statement(&self, s: &Statement, file_name: &str) -> rusqlite::Result<i64> {
        let mut conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction()?;

        tx.execute(
            "DELETE FROM statements WHERE issuer = ?1 AND holder = ?2 AND closing_on = ?3",
            params![s.issuer, s.holder, s.closing_on],
        )?;
        tx.execute(
            "INSERT INTO statements (issuer, holder, issued_on, due_on, closing_on, file_name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                s.issuer,
                s.holder,
                s.issued_on,
                s.due_on,
                s.closing_on,
                file_name
            ],
        )?;
        let statement_id = tx.last_insert_rowid();

        {
            let mut insert = tx.prepare(
                "INSERT INTO transactions (statement_id, position, date, description, amount_cents,
                     kind, card, installment_current, installment_total)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for (position, t) in s.transactions.iter().enumerate() {
                insert.execute(params![
                    statement_id,
                    position as i64,
                    t.date,
                    t.description,
                    t.amount_cents,
                    t.kind.as_str(),
                    t.card,
                    t.installment.map(|i| i.current),
                    t.installment.map(|i| i.total),
                ])?;
            }
        }

        tx.commit()?;
        Ok(statement_id)
    }

    /// Transactions of the statements due in the month of `month` (any day in it),
    /// ordered by date.
    pub fn month_transactions(&self, month: NaiveDate) -> rusqlite::Result<Vec<MonthTransaction>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT t.id, t.date, t.description, t.amount_cents, t.card,
                    t.installment_current, t.installment_total, s.issuer
             FROM transactions t
             JOIN statements s ON s.id = t.statement_id
             WHERE substr(s.due_on, 1, 7) = ?1
             ORDER BY t.date, s.id, t.position",
        )?;
        let rows = stmt.query_map(params![month.format("%Y-%m").to_string()], |r| {
            let current: Option<u16> = r.get(5)?;
            let total: Option<u16> = r.get(6)?;
            Ok(MonthTransaction {
                id: r.get(0)?,
                date: r.get(1)?,
                description: r.get(2)?,
                amount_cents: r.get(3)?,
                card: r.get(4)?,
                installment: current
                    .zip(total)
                    .map(|(current, total)| Installment { current, total }),
                issuer: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// Stores an income entry and returns its id.
    pub fn add_income(
        &self,
        year: i32,
        month: u32,
        description: &str,
        amount_cents: i64,
    ) -> rusqlite::Result<i64> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO incomes (year, month, description, amount_cents)
             VALUES (?1, ?2, ?3, ?4)",
            params![year, month, description, amount_cents],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// All income entries, newest month first.
    pub fn list_incomes(&self) -> rusqlite::Result<Vec<Income>> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, year, month, description, amount_cents
             FROM incomes
             ORDER BY year DESC, month DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Income {
                id: r.get(0)?,
                year: r.get(1)?,
                month: r.get(2)?,
                description: r.get(3)?,
                amount_cents: r.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Deletes an income entry. Returns whether it existed.
    pub fn delete_income(&self, id: i64) -> rusqlite::Result<bool> {
        let conn = self.0.lock().unwrap_or_else(|e| e.into_inner());
        Ok(conn.execute("DELETE FROM incomes WHERE id = ?1", params![id])? > 0)
    }
}

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (sql, n) in MIGRATIONS.iter().zip(1i64..).skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", n)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::statement::{Installment, Kind, Transaction};

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn sample() -> Statement {
        Statement {
            issuer: "Mercado Pago",
            holder: "FULANO DE TAL".into(),
            issued_on: date(2026, 10, 1),
            due_on: date(2026, 10, 10),
            closing_on: date(2026, 10, 3),
            transactions: vec![
                Transaction {
                    date: date(2026, 9, 5),
                    description: "LOJA".into(),
                    amount_cents: 12345,
                    kind: Kind::Purchase,
                    card: Some("1234".into()),
                    installment: Some(Installment {
                        current: 2,
                        total: 3,
                    }),
                },
                Transaction {
                    date: date(2026, 9, 10),
                    description: "Pagamento da fatura".into(),
                    amount_cents: -50000,
                    kind: Kind::Payment,
                    card: None,
                    installment: None,
                },
            ],
        }
    }

    fn count(db: &Db, table: &str) -> i64 {
        let conn = db.0.lock().unwrap();
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn reimport_replaces_previous_statement() {
        let db = Db::init(Connection::open_in_memory().unwrap()).unwrap();
        let s = sample();

        db.save_statement(&s, "a.pdf").unwrap();
        db.save_statement(&s, "b.pdf").unwrap();

        assert_eq!(count(&db, "statements"), 1);
        assert_eq!(count(&db, "transactions"), 2);

        let conn = db.0.lock().unwrap();
        let file: String = conn
            .query_row("SELECT file_name FROM statements", [], |r| r.get(0))
            .unwrap();
        assert_eq!(file, "b.pdf");

        let (kind, current, total): (String, Option<u16>, Option<u16>) = conn
            .query_row(
                "SELECT kind, installment_current, installment_total FROM transactions
                 WHERE position = 0",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (kind.as_str(), current, total),
            ("purchase", Some(2), Some(3))
        );
    }

    #[test]
    fn month_transactions_filters_by_due_month() {
        let db = Db::init(Connection::open_in_memory().unwrap()).unwrap();
        db.save_statement(&sample(), "a.pdf").unwrap();

        let rows = db.month_transactions(date(2026, 10, 1)).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].date, date(2026, 9, 5));
        assert_eq!(rows[0].issuer, "Mercado Pago");
        assert!(matches!(
            rows[0].installment,
            Some(Installment {
                current: 2,
                total: 3
            })
        ));
        assert!(rows[1].installment.is_none());

        assert!(db.month_transactions(date(2026, 9, 1)).unwrap().is_empty());
    }

    #[test]
    fn incomes_list_newest_month_first_and_delete() {
        let db = Db::init(Connection::open_in_memory().unwrap()).unwrap();
        let old = db.add_income(2026, 9, "Freela", 123456).unwrap();
        let salary = db.add_income(2026, 11, "Salário", 500000).unwrap();
        let bonus = db.add_income(2026, 11, "Bônus", 10000).unwrap();

        let ids: Vec<i64> = db.list_incomes().unwrap().iter().map(|i| i.id).collect();
        assert_eq!(ids, [bonus, salary, old]);

        assert!(db.delete_income(salary).unwrap());
        assert!(!db.delete_income(salary).unwrap());
        assert_eq!(count(&db, "incomes"), 2);
    }

    #[test]
    fn incomes_reject_invalid_values() {
        let db = Db::init(Connection::open_in_memory().unwrap()).unwrap();
        assert!(db.add_income(2026, 11, "Zero", 0).is_err());
        assert!(db.add_income(2026, 11, "Negativo", -1).is_err());
        assert!(db.add_income(2026, 13, "Mês inválido", 100).is_err());
    }
}
