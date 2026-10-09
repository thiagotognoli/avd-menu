//! Janelas X11 do emulador (no GNOME Wayland ele roda pelo XWayland).
//!
//! O GNOME (mutter) “pinga” a janela a cada clique (_NET_WM_PING) para mostrar
//! o aviso “não está respondendo”. Com o emulador isso dá errado: logo depois
//! de um clique o GNOME para de entregar o mouse à janela até o aviso aparecer
//! e alguém clicar em “Esperar” (mutter #3543) — e no Android o toque fica
//! preso. Desligar o aviso no sistema todo resolve
//! (`org.gnome.mutter check-alive-timeout 0`); aqui ele é desligado só para o
//! emulador: sem `_NET_WM_PING` em WM_PROTOCOLS o mutter não pinga a janela, e
//! ele relê essa propriedade quando ela muda.

#[cfg(target_os = "linux")]
mod imp {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode, Window};
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    /// Tira o _NET_WM_PING das janelas de primeiro nível dos processos `pids`.
    /// Devolve quantas janelas mudaram.
    pub fn disable_ping(pids: &[i32]) -> Result<usize, Box<dyn std::error::Error>> {
        if pids.is_empty() || std::env::var_os("DISPLAY").is_none() {
            return Ok(0);
        }
        let (conn, screen) = RustConnection::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let atom = |name: &str| -> Result<u32, Box<dyn std::error::Error>> { Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom) };
        let (protocols, ping, wm_pid) = (atom("WM_PROTOCOLS")?, atom("_NET_WM_PING")?, atom("_NET_WM_PID")?);
        let pid_of = |w: Window| -> Option<i32> {
            let r = conn.get_property(false, w, wm_pid, AtomEnum::CARDINAL, 0, 1).ok()?.reply().ok()?;
            let pid = r.value32()?.next().map(|v| v as i32);
            pid
        };
        // primeiro nível do XWayland; se o GNOME pôs a janela numa moldura, um nível abaixo
        let mut windows = vec![];
        for w in conn.query_tree(root)?.reply()?.children {
            if pid_of(w).is_some() {
                windows.push(w);
            } else if let Ok(t) = conn.query_tree(w)?.reply() {
                windows.extend(t.children);
            }
        }
        let mut changed = 0;
        for w in windows {
            if !pid_of(w).is_some_and(|p| pids.contains(&p)) {
                continue;
            }
            let Ok(r) = conn.get_property(false, w, protocols, AtomEnum::ATOM, 0, 64)?.reply() else { continue };
            let Some(list) = r.value32() else { continue };
            let list: Vec<u32> = list.collect();
            if !list.contains(&ping) {
                continue;
            }
            let keep: Vec<u32> = list.into_iter().filter(|a| *a != ping).collect();
            conn.change_property32(PropMode::REPLACE, w, protocols, AtomEnum::ATOM, &keep)?;
            changed += 1;
        }
        conn.flush()?;
        Ok(changed)
    }
}

#[cfg(target_os = "linux")]
pub use imp::disable_ping;

#[cfg(not(target_os = "linux"))]
pub fn disable_ping(_pids: &[i32]) -> Result<usize, Box<dyn std::error::Error>> {
    Ok(0)
}
