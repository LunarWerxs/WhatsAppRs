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
/// Asks a running instance to shut down cleanly rather than surface.
const QUIT: &[u8] = b"quit\n";

pub enum Instance {
    /// We are the only instance. Poll this for "raise the window" requests.
    First(TcpListener),
    /// Another instance already owns the port and has been told to show itself.
    Second,
}

pub fn acquire() -> Instance {
    // WHATSAPP_RS_INSTANCE_PORT: a test instance takes its own lock, so it can
    // run beside the real one instead of just waking it up.
    let port = std::env::var("WHATSAPP_RS_INSTANCE_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(PORT);
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
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

/// What a second launch, or a maintenance script, asked the running app to do.
#[derive(Default)]
pub struct Requests {
    /// A second launch happened: bring the window forward.
    pub show: bool,
    /// Shut down cleanly, exactly as the tray's Quit does.
    ///
    /// This exists because a killed process never writes its cookie jar, and on
    /// Servo that costs the WhatsApp login. Restarting the app to pick up a new
    /// build should not make the user scan a QR code again.
    pub quit: bool,
}

/// Drain anything queued on the lock port. A burst of launches raises the window once.
pub fn poll_requests(listener: &TcpListener) -> Requests {
    let mut requests = Requests::default();
    while let Ok((mut stream, _)) = listener.accept() {
        let mut buf = [0u8; 16];
        let read = stream.read(&mut buf).unwrap_or(0);
        if buf[..read].starts_with(QUIT) {
            requests.quit = true;
        } else {
            requests.show = true;
        }
    }
    requests
}

/// Drain any pending "show" requests. Returns true if another launch asked us to surface.
pub fn poll_show_request(listener: &TcpListener) -> bool {
    poll_requests(listener).show
}

/// Ask a running instance to quit cleanly. False if nothing was listening.
pub fn request_quit(port: Option<u16>) -> bool {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port.unwrap_or(PORT));
    match TcpStream::connect(addr) {
        Ok(mut stream) => {
            let _ = stream.write_all(QUIT);
            let _ = stream.flush();
            true
        }
        Err(_) => false,
    }
}
