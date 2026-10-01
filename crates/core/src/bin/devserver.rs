//! Servidor HTTP de DESENVOLVIMENTO: expõe a mesma API que o shell do Tauri
//! entrega à interface, para abrir a interface num navegador (e testá-la com
//! Chrome headless) sem precisar compilar o app inteiro.
//!
//!   cargo run -p avdcore --features devserver --bin devserver -- --port 18080 --ui ui

use avdcore::api::App;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Events {
    seq: u64,
    queue: Vec<(u64, String, Value)>,
}

fn main() {
    let mut port = 18080u16;
    let mut ui = PathBuf::from("ui");
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--port" => port = it.next().and_then(|v| v.parse().ok()).unwrap_or(port),
            "--ui" => ui = it.next().map(PathBuf::from).unwrap_or(ui),
            "--sdk" => {
                if let Some(p) = it.next() {
                    avdcore::sdk::set_override(&p);
                }
            }
            _ => {}
        }
    }
    let events = Arc::new(Mutex::new(Events { seq: 0, queue: vec![] }));
    let ev = events.clone();
    let app = App::new(move |name, data| {
        let mut e = ev.lock().unwrap();
        e.seq += 1;
        let s = e.seq;
        e.queue.push((s, name.to_string(), data));
        if e.queue.len() > 500 {
            e.queue.drain(..100);
        }
    });
    app.set_quit(|| std::process::exit(0));
    app.spawn_poller();
    let server = tiny_http::Server::http(("127.0.0.1", port)).expect("porta ocupada");
    eprintln!("devserver em http://127.0.0.1:{port}/  (ui: {})", ui.display());
    for req in server.incoming_requests() {
        let (app, events, ui) = (app.clone(), events.clone(), ui.clone());
        std::thread::spawn(move || handle(app, events, ui, req));
    }
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

fn respond(req: tiny_http::Request, code: u16, ctype: &str, body: Vec<u8>) {
    let h = tiny_http::Header::from_bytes("Content-Type", ctype).unwrap();
    let nc = tiny_http::Header::from_bytes("Cache-Control", "no-store").unwrap();
    let _ = req.respond(tiny_http::Response::from_data(body).with_status_code(code).with_header(h).with_header(nc));
}

fn handle(app: Arc<App>, events: Arc<Mutex<Events>>, ui: PathBuf, mut req: tiny_http::Request) {
    let url = req.url().to_string();
    let method = req.method().as_str().to_string();
    if url.starts_with("/api/events") {
        // long-poll: devolve os eventos novos desde `since` (espera até 20 s)
        let since: u64 = url.split("since=").nth(1).and_then(|v| v.split('&').next()).and_then(|v| v.parse().ok()).unwrap_or(0);
        for _ in 0..200 {
            {
                let e = events.lock().unwrap();
                let fresh: Vec<_> = e.queue.iter().filter(|(s, _, _)| *s > since).map(|(s, n, d)| json!({"seq": s, "event": n, "data": d})).collect();
                if !fresh.is_empty() || since == 0 {
                    let body = json!({"seq": e.seq, "events": fresh});
                    return respond(req, 200, "application/json", body.to_string().into_bytes());
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let seq = events.lock().unwrap().seq;
        return respond(req, 200, "application/json", json!({"seq": seq, "events": []}).to_string().into_bytes());
    }
    if url.starts_with("/api/") {
        let mut raw = String::new();
        let _ = req.as_reader().read_to_string(&mut raw);
        let body: Value = if raw.trim().is_empty() { Value::Null } else { serde_json::from_str(&raw).unwrap_or(Value::Null) };
        return match app.handle(&method, &url, body) {
            Ok(v) => respond(req, 200, "application/json", v.to_string().into_bytes()),
            Err(e) => {
                let mut b = json!({"error": e.message});
                if let Some(d) = e.data {
                    b["data"] = d;
                }
                respond(req, e.status.max(400), "application/json", b.to_string().into_bytes())
            }
        };
    }
    let rel = url.split('?').next().unwrap_or("/").trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };
    if rel.contains("..") {
        return respond(req, 400, "text/plain", b"bad path".to_vec());
    }
    match std::fs::read(ui.join(rel)) {
        Ok(b) => respond(req, 200, mime(rel), b),
        Err(_) => respond(req, 404, "text/plain", b"not found".to_vec()),
    }
}
