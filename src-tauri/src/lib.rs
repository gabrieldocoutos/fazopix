mod db;
mod money;
mod parsers;
mod statement;

use chrono::NaiveDate;
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
struct UploadSummary {
    statement_id: i64,
    transactions: usize,
}

/// Lists the transactions of the faturas due in `month` (`"YYYY-MM"`).
#[tauri::command(async)]
fn list_transactions(
    month: String,
    db: tauri::State<'_, db::Db>,
) -> Result<Vec<db::MonthTransaction>, String> {
    let first_day = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d")
        .map_err(|e| format!("invalid month {month}: {e}"))?;
    db.month_transactions(first_day)
        .map_err(|e| format!("failed to list transactions for {month}: {e}"))
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Receives an invoice PDF as a raw IPC body.
/// The original file name comes URI-encoded in the `x-file-name` header.
#[tauri::command(async)]
fn upload_fatura(
    request: tauri::ipc::Request<'_>,
    db: tauri::State<'_, db::Db>,
) -> Result<UploadSummary, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected a raw body".into());
    };
    let raw_name = request
        .headers()
        .get("x-file-name")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    let name = urlencoding::decode(raw_name)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| raw_name.to_string());

    // TODO: stop hardcoding; ask the user and store it in the keychain.
    const PDF_PASSWORD: &str = "03941";

    // Falls back to the known password when the PDF is password-protected.
    let text = pdf_extract::extract_text_from_mem(bytes)
        .or_else(|err| extract_encrypted_text(bytes, PDF_PASSWORD).map_err(|_| err))
        .map_err(|e| format!("failed to read PDF {name}: {e}"))?;

    println!("{}", text);

    let statement = parsers::parse(&text).map_err(|e| format!("failed to parse {name}: {e}"))?;

    println!("--- {name} ({} bytes) ---\n{statement}", bytes.len());

    let statement_id = db
        .save_statement(&statement, &name)
        .map_err(|e| format!("failed to save {name}: {e}"))?;

    Ok(UploadSummary {
        statement_id,
        transactions: statement.transactions.len(),
    })
}

/// Extracts text from a password-protected PDF.
///
/// `pdf_extract::extract_text_from_mem_encrypted` can't be used: it loads with
/// `Document::load_mem` and decrypts afterwards, but lopdf >= 0.42 skips encrypted
/// objects unless the password is given at load time, so it returns empty text.
fn extract_encrypted_text(
    bytes: &[u8],
    password: &str,
) -> Result<String, pdf_extract::OutputError> {
    let doc = pdf_extract::Document::load_mem_with_options(
        bytes,
        pdf_extract::LoadOptions::with_password(password),
    )?;
    let mut text = String::new();
    pdf_extract::output_doc(&doc, &mut pdf_extract::PlainTextOutput::new(&mut text))?;
    Ok(text)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(db::Db::open(&dir.join("facaopix.db"))?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            upload_fatura,
            list_transactions
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
