mod money;
mod parsers;
mod statement;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Receives an invoice PDF as a raw IPC body.
/// The original file name comes URI-encoded in the `x-file-name` header.
#[tauri::command(async)]
fn upload_fatura(request: tauri::ipc::Request<'_>) -> Result<(), String> {
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

    let text = pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| format!("failed to read PDF {name}: {e}"))?;

    let statement = parsers::parse(&text).map_err(|e| format!("failed to parse {name}: {e}"))?;

    println!("--- {name} ({} bytes) ---\n{statement}", bytes.len());
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, upload_fatura])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
