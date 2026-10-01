//! Shell do AVD Menu: abre a janela (WebView nativo do sistema) e entrega à
//! interface o núcleo em Rust (`avdcore`). Sem servidor HTTP: a interface chama
//! o comando `api` e recebe eventos pelo canal “avd-event”.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use avdcore::api::App;
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use tauri::{Emitter, Manager};

static HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Ponte única da interface: método + caminho (com query) + corpo JSON.
#[tauri::command]
async fn api(app: tauri::State<'_, Arc<App>>, method: String, path: String, body: Option<Value>) -> Result<Value, String> {
    let app = app.inner().clone();
    tauri::async_runtime::spawn_blocking(move || app.handle(&method, &path, body.unwrap_or(Value::Null)))
        .await
        .map_err(|e| json!({"error": e.to_string(), "status": 500}).to_string())?
        .map_err(|e| json!({"error": e.message, "status": e.status, "data": e.data}).to_string())
}

fn main() {
    // Linha de comando (list, start, sdk install...) roda sem abrir janela.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = avdcore::cli::run(&args) {
        std::process::exit(code);
    }

    // O renderizador dmabuf do WebKitGTK deixa a janela em branco em alguns
    // drivers (NVIDIA / Wayland / AppImage); para esta interface não faz diferença.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let core = App::new(|event, data| {
        if let Some(h) = HANDLE.get() {
            let _ = h.emit("avd-event", json!({"event": event, "data": data}));
        }
    });
    core.spawn_poller();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Segunda execução: só traz a janela existente para a frente.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .manage(core.clone())
        .invoke_handler(tauri::generate_handler![api])
        .setup(move |app| {
            let _ = HANDLE.set(app.handle().clone());
            let h = app.handle().clone();
            core.set_quit(move || h.exit(0));
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("AVD Menu: {e}");
        std::process::exit(1);
    }
}
