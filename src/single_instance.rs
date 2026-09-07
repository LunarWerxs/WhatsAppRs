//! Single-instance guard, cross-platform with no dependencies.
//!
//! Binds a loopback TCP port. Binding succeeds for exactly one process, so the bind
//! itself is the lock, and it is released by the OS even if the app is killed, which
//! a lock file is not. A second launch connects and says "show", then exits.
//!
//! The C# original used a named mutex plus a broadcast window message. That is two
//! Windows-only mechanisms; this is one portable one.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};

/// Arbitrary high port. Loopback only, so it is never exposed off the machine.
const PORT: u16 = 47_913;
const SHOW: &[u8] = b"show\n";

pub enum Instance {
    /// We are the only instance. Poll this for "raise the window" requests.
    First(TcpListener),
    /// Another instance already owns the port and has been told to show itself.
    Second,
}

pub fn acquire() -> Instance {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, PORT);
    match TcpListener::bind(addr) {
        Ok(listener) => {
            // Non-blocking so the event loop can poll without stalling.
            let _ = listener.set_nonblocking(true);
            Instance::First(listener)
        }
        Err(_) => {
            // Someone holds the port. Ask them to surface, then bow out.
            if let Ok(mut stream) = TcpStream::connect(addr) {
                let _ = stream.write_all(SHOW);
                let _ = stream.flush();
            }
            Instance::Second
        }
    }
}

/// Drain any pending "show" requests. Returns true if another launch asked us to surface.
pub fn poll_show_request(listener: &TcpListener) -> bool {
    let mut asked = false;
    // Accept everything queued; a burst of launches should raise the window once.
    while let Ok((mut stream, _)) = listener.accept() {
        let mut buf = [0u8; 16];
        let _ = stream.read(&mut buf);
        asked = true;
    }
    asked
}
