//! One running copy: a second launch asks the first to show its window (or quit) over a socket.
use crate::worker::Core;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;

fn path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    dir.join("f75-studio.sock")
}

/// True if another copy is running (and got the message).
pub fn signal(msg: &str) -> bool {
    match UnixStream::connect(path()) {
        Ok(mut s) => s.write_all(msg.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

pub fn listen(core: Arc<Core>) {
    let p = path();
    let _ = std::fs::remove_file(&p); // stale socket from a crash (signal() already failed)
    let Ok(l) = UnixListener::bind(&p) else { return };
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut msg = String::new();
            let _ = s.read_to_string(&mut msg);
            match msg.trim() {
                "quit" => core.request_quit(),
                "ping" => {}
                _ => core.request_show(),
            }
        }
    });
}
