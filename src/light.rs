//! Light mode: WhatsApp's protocol spoken directly, with no browser engine.
//!
//! Measured at about 27 MB working set in one process (3.7 MB private), against
//! about 425 MB for safe mode. That saving is the entire reason this mode exists.
//!
//! ⚠ The protocol is not published. It is known only because people reverse
//! engineered WhatsApp's own applications, which their Terms of Service forbid,
//! and accounts using such clients have been permanently banned with no appeal.
//! `mode.rs` makes the user choose this deliberately; nothing here should ever
//! run without that explicit choice.
//!
//! Shape: the protocol client runs on its own thread inside a two-worker tokio
//! runtime and talks to the window through [`Sink`] (events up) and an
//! [`Outgoing`] channel (sends down). On Windows the window is `chat_ui.rs`,
//! plain Win32 controls so the footprint stays where it is. Elsewhere there is no
//! window yet: the QR prints to the console and messages arrive as notifications.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use whatsapp_rust::prelude::*;

use crate::chats::{friendly_jid, Chats, Msg};
use crate::{notify, paths};

/// Session keys and app state. Deleting it unlinks the device.
const STORE: &str = "light-session.db";

/// What the protocol side tells the window.
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// A fresh pairing code to render as a QR. Short-lived; never persisted.
    Qr(String),
    Connected,
    LoggedOut(String),
    /// The store changed: a message arrived, a name was learned, history synced.
    ChatsChanged,
    Status(String),
}

/// What the window asks the protocol side to send.
#[derive(Debug)]
pub struct Outgoing {
    pub jid: String,
    pub text: String,
}

/// The window's side of the conversation. Must be callable from any thread.
pub trait Sink: Send + Sync + 'static {
    fn event(&self, e: UiEvent);
    /// True while the user is looking at the window, so a toast would be noise.
    fn is_watching(&self) -> bool {
        false
    }
}

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = paths::data_dir();
    // Toasts need this identity, and the shortcut that backs it, before the first one.
    notify::set_app_user_model_id(crate::APP_ID);
    let _ = crate::shortcut::ensure(crate::APP_ID);
    // `--light-demo`: the window with sample chats and no network, so the UI can be
    // looked at without linking a device. The real store is never touched.
    let demo = std::env::args().any(|a| a == "--light-demo");
    let chats = Arc::new(Mutex::new(if demo {
        Chats::ephemeral()
    } else {
        Chats::load(&data_dir)
    }));

    #[cfg(target_os = "windows")]
    {
        crate::chat_ui::run(data_dir, chats, demo)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = demo;
        headless(data_dir, chats)
    }
}

/// The demo's stand-in for the protocol thread: seeds a few chats, answers every
/// send with a canned reply a moment later. Ends when the window drops the channel.
pub fn spawn_demo(
    sink: Arc<dyn Sink>,
    chats: Arc<Mutex<Chats>>,
    mut outgoing: tokio::sync::mpsc::UnboundedReceiver<Outgoing>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("whatsapp-light-demo".into())
        .spawn(move || {
            let base = now().saturating_sub(3600);
            let seed: &[(&str, &str, bool, &[(bool, &str, u64, &str)])] = &[
                (
                    "15551234567@s.whatsapp.net",
                    "Sam",
                    false,
                    &[
                        (false, "Sam", 0, "Are we still on for tonight?"),
                        (true, "", 120, "Yes. 7 at the usual place."),
                        (false, "Sam", 200, "Perfect, see you there."),
                    ],
                ),
                (
                    "120363012345678901@g.us",
                    "Weekend hike",
                    true,
                    &[
                        (false, "Priya", 300, "Trailhead parking fills by 8, leave early."),
                        (false, "Marcus", 420, "I can drive, room for three."),
                        (true, "", 500, "Count me in."),
                    ],
                ),
                (
                    "15559876543@s.whatsapp.net",
                    "Mum",
                    false,
                    &[(false, "Mum", 900, "Call me when you get a minute.")],
                ),
            ];
            {
                let mut c = chats.lock().unwrap();
                for (jid, name, is_group, msgs) in seed {
                    c.name_chat(jid, *is_group, name);
                    for (i, (from_me, sender, offset, text)) in msgs.iter().enumerate() {
                        c.push(
                            jid,
                            *is_group,
                            Msg {
                                id: format!("demo-{jid}-{i}"),
                                from_me: *from_me,
                                sender: sender.to_string(),
                                ts: base + offset,
                                text: text.to_string(),
                            },
                            !from_me,
                        );
                    }
                }
            }
            sink.event(UiEvent::Connected);
            sink.event(UiEvent::ChatsChanged);

            let mut n = 0u32;
            while let Some(out) = outgoing.blocking_recv() {
                n += 1;
                let is_group = out.jid.ends_with("@g.us");
                chats.lock().unwrap().push(
                    &out.jid,
                    is_group,
                    Msg {
                        id: format!("demo-sent-{n}"),
                        from_me: true,
                        sender: String::new(),
                        ts: now(),
                        text: out.text.clone(),
                    },
                    false,
                );
                sink.event(UiEvent::ChatsChanged);
                std::thread::sleep(std::time::Duration::from_millis(800));
                let reply = format!("(demo) got: {}", out.text);
                chats.lock().unwrap().push(
                    &out.jid,
                    is_group,
                    Msg {
                        id: format!("demo-reply-{n}"),
                        from_me: false,
                        sender: "Demo".into(),
                        ts: now(),
                        text: reply.clone(),
                    },
                    true,
                );
                sink.event(UiEvent::ChatsChanged);
                if !sink.is_watching() {
                    notify::toast("Demo", &reply);
                }
            }
        })
        .expect("spawn demo thread")
}

/// Start the protocol client on its own thread. Returns when `shutdown` fires.
pub fn spawn_bot(
    data_dir: PathBuf,
    sink: Arc<dyn Sink>,
    chats: Arc<Mutex<Chats>>,
    outgoing: tokio::sync::mpsc::UnboundedReceiver<Outgoing>,
    shutdown: Arc<tokio::sync::Notify>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("whatsapp-light".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                // A messaging client waits on the network; it does not compute. The
                // default of one worker per core would be 32 threads of stacks and
                // allocator arenas on this machine, most of what this mode saves.
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    sink.event(UiEvent::Status(format!("Could not start: {e}")));
                    return;
                }
            };
            if let Err(e) = runtime.block_on(serve(&data_dir, sink.clone(), chats, outgoing, shutdown))
            {
                sink.event(UiEvent::Status(format!("Stopped: {e}")));
            }
        })
        .expect("spawn light-mode thread")
}

async fn serve(
    data_dir: &Path,
    sink: Arc<dyn Sink>,
    chats: Arc<Mutex<Chats>>,
    mut outgoing: tokio::sync::mpsc::UnboundedReceiver<Outgoing>,
    shutdown: Arc<tokio::sync::Notify>,
) -> Result<(), Box<dyn std::error::Error>> {
    let store_path = data_dir.join(STORE);
    let first_link = !store_path.exists();

    let bot = Bot::builder()
        .with_backend(SqliteStore::new(store_path.to_string_lossy().as_ref()).await?)
        .on_qr_code({
            let sink = sink.clone();
            move |code, _timeout| {
                let sink = sink.clone();
                async move { sink.event(UiEvent::Qr(code)) }
            }
        })
        .on_connected({
            let sink = sink.clone();
            move |_client| {
                let sink = sink.clone();
                async move { sink.event(UiEvent::Connected) }
            }
        })
        .on_logged_out({
            let sink = sink.clone();
            move |info| {
                let sink = sink.clone();
                async move { sink.event(UiEvent::LoggedOut(format!("{:?}", info.reason))) }
            }
        })
        .on_message({
            let sink = sink.clone();
            let chats = chats.clone();
            move |ctx| {
                let sink = sink.clone();
                let chats = chats.clone();
                async move { on_message(&ctx, &chats, sink.as_ref()) }
            }
        })
        .on_event({
            let sink = sink.clone();
            let chats = chats.clone();
            move |event, _client| {
                let sink = sink.clone();
                let chats = chats.clone();
                async move { on_event(&event, &chats, sink.as_ref()) }
            }
        })
        .build()
        .await?;

    let client = bot.client();
    let handle = bot.spawn();
    sink.event(UiEvent::Status(
        if first_link {
            "Waiting for you to scan the pairing code."
        } else {
            "Connecting."
        }
        .into(),
    ));

    // Sends. One task, in order, so a burst of Enter presses arrives as typed.
    let sender = {
        let sink = sink.clone();
        let chats = chats.clone();
        tokio::spawn(async move {
            while let Some(out) = outgoing.recv().await {
                let jid: Jid = match out.jid.parse() {
                    Ok(j) => j,
                    Err(e) => {
                        sink.event(UiEvent::Status(format!("Bad address {}: {e}", out.jid)));
                        continue;
                    }
                };
                match client.send_text(jid, out.text.clone()).await {
                    Ok(res) => {
                        let is_group = out.jid.ends_with("@g.us");
                        let msg = Msg {
                            id: res.message_id,
                            from_me: true,
                            sender: String::new(),
                            ts: now(),
                            text: out.text,
                        };
                        chats.lock().unwrap().push(&out.jid, is_group, msg, false);
                        sink.event(UiEvent::ChatsChanged);
                    }
                    Err(e) => sink.event(UiEvent::Status(format!("Send failed: {e}"))),
                }
            }
        })
    };

    shutdown.notified().await;
    sender.abort();
    handle.shutdown().await;
    Ok(())
}

fn on_message(ctx: &MessageContext, chats: &Mutex<Chats>, sink: &dyn Sink) {
    let info = &ctx.info;
    let jid = info.source.chat.to_string();
    let is_group = info.source.is_group;
    let from_me = info.source.is_from_me;
    let text = message_text(&ctx.message);

    let sender = if from_me {
        String::new()
    } else if !info.push_name.is_empty() {
        info.push_name.clone()
    } else {
        chats.lock().unwrap().display_name(&info.source.sender.to_string())
    };
    let msg = Msg {
        id: info.id.to_string(),
        from_me,
        sender,
        ts: info.timestamp.timestamp().max(0) as u64,
        text: text.clone(),
    };
    let added = chats.lock().unwrap().push(&jid, is_group, msg, !from_me);
    if !added {
        return;
    }
    sink.event(UiEvent::ChatsChanged);
    if !from_me && !sink.is_watching() {
        let title = chats.lock().unwrap().display_name(&jid);
        notify::toast(&title, &truncate(&text, 120));
    }
}

fn on_event(event: &Event, chats: &Mutex<Chats>, sink: &dyn Sink) {
    match event {
        Event::HistorySync(hs) => {
            let Some(sync) = hs.get() else { return };
            let mut c = chats.lock().unwrap();
            for conv in &sync.conversations {
                import_conversation(&mut c, conv);
            }
            drop(c);
            sink.event(UiEvent::ChatsChanged);
        }
        Event::ContactUpdate(cu) => {
            let name = cu
                .action
                .full_name
                .clone()
                .or_else(|| cu.action.first_name.clone());
            if let Some(name) = name {
                chats
                    .lock()
                    .unwrap()
                    .set_contact_name(&cu.jid.to_string(), &name);
                sink.event(UiEvent::ChatsChanged);
            }
        }
        Event::PushNameUpdate(p) => {
            // A push name is what the peer calls themselves; it only fills a blank.
            let jid = p.jid.to_string();
            let mut c = chats.lock().unwrap();
            if c.get(&jid).map(|ch| ch.name.is_empty()).unwrap_or(false) {
                c.name_chat(&jid, false, &p.new_push_name);
                drop(c);
                sink.event(UiEvent::ChatsChanged);
            }
        }
        _ => {}
    }
}

/// One conversation out of the phone's history sync, into the store.
fn import_conversation(chats: &mut Chats, conv: &wa::Conversation) {
    let jid = conv.id.clone();
    let is_group = jid.ends_with("@g.us");
    if let Some(name) = conv.name.as_deref() {
        chats.name_chat(&jid, is_group, name);
    }
    for hm in &conv.messages {
        let Some(w) = hm.message.as_option() else { continue };
        let Some(key) = w.key.as_option() else { continue };
        // Only text survives into the store; media needs a download pipeline this
        // mode does not have yet.
        let Some(text) = w.message.as_option().and_then(|m| {
            m.text_content()
                .or_else(|| m.get_caption())
                .map(str::to_string)
        }) else {
            continue;
        };
        let from_me = key.from_me.unwrap_or(false);
        let sender = if from_me {
            String::new()
        } else {
            w.push_name
                .clone()
                .filter(|s| !s.is_empty())
                .or_else(|| key.participant.as_deref().map(friendly_jid))
                .unwrap_or_default()
        };
        chats.push(
            &jid,
            is_group,
            Msg {
                id: key.id.clone().unwrap_or_default(),
                from_me,
                sender,
                ts: w.message_timestamp.unwrap_or(0),
                text,
            },
            false,
        );
    }
}

fn message_text(message: &wa::Message) -> String {
    message
        .text_content()
        .or_else(|| message.get_caption())
        .map(str::to_string)
        .unwrap_or_else(|| "[attachment]".to_string())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// No window on this platform yet: QR on the console, messages as notifications.
#[cfg(not(target_os = "windows"))]
fn headless(data_dir: PathBuf, chats: Arc<Mutex<Chats>>) -> Result<(), Box<dyn std::error::Error>> {
    struct Console;
    impl Sink for Console {
        fn event(&self, e: UiEvent) {
            match e {
                UiEvent::Qr(code) => println!(
                    "\n=== Scan with WhatsApp: Linked Devices -> Link a Device ===\n{}",
                    render_qr(&code)
                ),
                UiEvent::Connected => println!("connected"),
                UiEvent::LoggedOut(r) => println!("logged out: {r}"),
                UiEvent::Status(s) => println!("{s}"),
                UiEvent::ChatsChanged => {}
            }
        }
    }
    let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let shutdown = Arc::new(tokio::sync::Notify::new());
    let bot = spawn_bot(data_dir, Arc::new(Console), chats.clone(), rx, shutdown.clone());
    let waiter = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    waiter.block_on(tokio::signal::ctrl_c())?;
    shutdown.notify_one();
    let _ = bot.join();
    chats.lock().unwrap().save_if_due(true);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn render_qr(code: &str) -> String {
    use qrcode::render::unicode;
    use qrcode::{EcLevel, QrCode};
    match QrCode::with_error_correction_level(code, EcLevel::L) {
        Ok(qr) => qr
            .render::<unicode::Dense1x2>()
            .quiet_zone(true)
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .build(),
        Err(err) => format!("(could not render QR: {err})\nraw pairing string:\n{code}"),
    }
}
