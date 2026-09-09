//! The app watching itself, because on 2026-09-09 it did not.
//!
//! At 04:10:37 an NVIDIA driver reset took the D3D11 device out from under the engine.
//! Chromium's GPU thread began rebuilding the context and never stopped: about seven
//! thousand `eglCreateContext` attempts a second, for eight hours and forty-five minutes.
//! By the time anyone looked, `cef.log` was **243,629,516,234 bytes** - 244 GB, growing at
//! 8.54 MB/s - the browser process held 10.9 GB private and was gaining 1.18 GB/hour, and
//! one thread had burned 8.3 CPU-hours. The system drive was down to 7.4% free and about
//! two hours from full.
//!
//! `cef_view.rs` no longer ships `--in-process-gpu`, which is what removed Chromium's own
//! circuit breaker and is the actual cause; that fix is the one that matters. This file is
//! the backstop for the NEXT runaway, whatever causes it, and it exists because of the one
//! sentence in the incident report that is worse than any of the numbers:
//!
//! > It was found because the owner noticed WhatsApp using 12 GB of RAM, not because
//! > anything in the app reported it.
//!
//! So there are two jobs here, and deliberately only two:
//!
//! 1. **Bound the log.** CEF neither rotates nor caps its log file, and the app points it
//!    at the user's system drive. A cap alone would have turned a near-outage into a
//!    non-event: 244 GB becomes 32 MB.
//! 2. **Say so.** One toast per reason, once per run, the first time the app notices itself
//!    misbehaving - by the rate its log is growing (checked every tick, because a firehose
//!    fills a disk in hours) or by the private bytes of its whole process tree (checked every
//!    twentieth, because a leak takes hours and the check is not free).
//!
//! **What this deliberately does NOT do is restart or quit the app.** A messaging client
//! that shuts itself down on a heuristic is worse than one that runs hot, and killing
//! Chromium skips its cookie flush, which is how the WhatsApp login gets lost and the phone
//! has to re-pair (see `main.rs`). With the GPU switch gone Chromium falls back to software
//! rendering by itself and the app keeps working, degraded; the watchdog's job is to make
//! sure that if something new goes wrong, the disk survives it and a human hears about it
//! inside a minute instead of nine hours.

use std::path::{Path, PathBuf};

use crate::notify;

/// The live log's ceiling. Past this it is rolled back to empty.
///
/// 16 MiB against a Chromium log that writes kilobytes an hour at `WARNING` in normal use,
/// and 12.8 MB per 1.5 s tick during the incident. Small enough that the worst case - this
/// plus one onset snapshot plus a tick's overshoot - is about 45 MB, and large enough that
/// a genuinely chatty debugging run (`WHATSAPP_RS_CEF_SWITCHES=v=3`) still has room to be
/// useful before it wraps.
const LOG_CAP: u64 = 16 * 1024 * 1024;

/// A tick that adds this much to the log is not logging, it is a fault loop.
///
/// 2 MiB over a 1.5 s tick is 1.4 MB/s. The incident ran at 8.54 MB/s, six times this;
/// normal operation is indistinguishable from zero. The margin either side is what makes
/// the threshold defensible without tuning.
const FIREHOSE_TICK_BYTES: u64 = 2 * 1024 * 1024;

/// ...and it has to keep it up for this many ticks, so a burst of startup warnings or one
/// noisy page load cannot raise a false alarm. Four ticks is six seconds.
const FIREHOSE_TICKS: u32 = 4;

/// Private bytes across the whole process tree that mean something is wrong.
///
/// Logged out the tree measures 350-371 MB private (bench, 2026-09-09). Logged in with a
/// real account and a full chat list the owner sees about 890 MB of working set, so call a
/// heavy but healthy tree 1 GB of private commit. The fault reached 10.9 GB in one process
/// alone. 3 GiB sits three times above the heaviest honest reading and three and a half
/// times below the observed fault, and at the incident's measured 1.18 GB/hour it is
/// reached about two hours in - against the eight and three quarter hours it actually ran.
const PRIVATE_ALARM: u64 = 3 * 1024 * 1024 * 1024;

/// Memory is sampled every this many ticks - 30 seconds - and not every one.
///
/// Reading it means a `TH32CS_SNAPPROCESS` snapshot of every process on the machine plus an
/// `OpenProcess` per child. That is a millisecond or two on a busy box, which is nothing
/// once and about 0.1% of a core forever at 1.5 s intervals. This app's entire argument is
/// that it is the light way to run WhatsApp; it does not get to burn a tenth of a percent
/// watching itself. Nothing is lost: the fault leaked 1.18 GB an hour, so half a minute of
/// granularity is 20 MB.
const MEMORY_EVERY_TICKS: u64 = 20;

/// ...and the ceiling has to be over on two consecutive samples before it counts. A process
/// id can be recycled between the snapshot and the `OpenProcess` that reads it, so a single
/// sample can in principle measure a stranger; a minute of it cannot.
const MEMORY_BREACHES: u32 = 2;

/// Consecutive failed attempts to empty the log before the user is told. Three is about
/// five seconds, which rules out a transient sharing conflict without sitting on a real one.
const ROLL_FAILURES: u32 = 3;

/// A floor under `WHATSAPP_RS_LOG_CAP_BYTES`. A cap of 0 or 1 would roll the log on every
/// tick forever, which is a runaway of its own shape.
const MIN_LOG_CAP: u64 = 64 * 1024;

/// The engine's log. Named here rather than in `cef_view.rs` so the writer of the path and
/// the watcher of it cannot drift apart.
pub fn log_path(data_dir: &Path) -> PathBuf {
    data_dir.join("cef.log")
}

/// The first `LOG_CAP` of the most recent run that went wrong, kept when the live log wraps.
///
/// This file is the whole reason the 2026-09-09 incident has a root cause: the diagnosis
/// came from the *head* of the log - the five lines at 04:10:42 where the device was first
/// reported removed - and a plain truncate-to-zero destroys exactly that. The tail is
/// always in `cef.log`; the onset is only ever available once.
///
/// It **outlives the process**. See [`Watchdog::arm`] for why that is not an oversight.
fn onset_path(data_dir: &Path) -> PathBuf {
    data_dir.join("cef.log.onset")
}

pub struct Watchdog {
    log: PathBuf,
    onset: PathBuf,
    cap: u64,
    private_alarm: u64,
    /// The log's length at the previous tick, so growth can be measured. A roll resets it
    /// to zero, which is why the growth calculation saturates rather than wrapping.
    last_len: u64,
    firehose_ticks: u32,
    /// Ticks since arming, so memory can be sampled on a slower cadence than the log.
    ticks: u64,
    /// The most recent memory reading, kept so a roll can report it without taking a fresh
    /// process snapshot at the worst possible moment.
    last_private: Option<u64>,
    memory_breaches: u32,
    /// Consecutive failures to empty the log, and whether that has been reported.
    roll_failures: u32,
    alarmed_roll: bool,
    onset_saved: bool,
    /// One alarm per run **per reason**. A watchdog that toasts every tick is a second
    /// runaway; a watchdog whose memory alarm eats the log alarm's only slot is a watchdog
    /// that reports the symptom it happened to see first.
    alarmed_log: bool,
    alarmed_memory: bool,
    /// Whether it has been said, once, that the memory figure is short. Not a toast: a
    /// degraded probe is a diagnostic problem, not something to interrupt a person over,
    /// and the log-rate sensor - the one that actually caught 2026-09-09 - is unaffected.
    blind_reported: bool,
}

impl Watchdog {
    /// Arm before `cef::initialize`, and only in the browser process.
    ///
    /// `cef.log` is cleared, because it is this run's log and nothing is holding it yet -
    /// this is the one moment a delete of it can succeed at all; see [`Watchdog::roll`] for
    /// why it never can afterwards.
    ///
    /// ⛔ **`cef.log.onset` is deliberately NOT cleared.** An earlier version of this cleared
    /// both, so that the existence of the onset file meant "*this run* went wrong". That was
    /// self-defeating and would have thrown away the only thing worth keeping: both alarms
    /// this file raises end with "Quit and reopen WhatsApp", so a user who does exactly what
    /// they are told deletes the evidence of the fault they were just told about. The onset
    /// therefore survives restarts and is overwritten only by the next real fault, which
    /// makes it "the most recent onset" rather than "this run's" - and Chromium stamps every
    /// line with a date and time, so the file says for itself when it happened.
    pub fn arm(data_dir: &Path) -> Self {
        Self::armed(
            data_dir,
            env_bytes("WHATSAPP_RS_LOG_CAP_BYTES")
                .map(|n| n.max(MIN_LOG_CAP))
                .unwrap_or(LOG_CAP),
            env_bytes("WHATSAPP_RS_PRIVATE_ALARM_BYTES").unwrap_or(PRIVATE_ALARM),
        )
    }

    /// The thresholds passed in rather than read, so a test states them instead of setting
    /// process-global environment variables that the next test would inherit.
    fn armed(data_dir: &Path, cap: u64, private_alarm: u64) -> Self {
        let log = log_path(data_dir);
        let onset = onset_path(data_dir);
        let _ = std::fs::remove_file(&log);
        Self {
            log,
            onset,
            cap,
            private_alarm,
            last_len: 0,
            firehose_ticks: 0,
            ticks: 0,
            last_private: None,
            memory_breaches: 0,
            roll_failures: 0,
            alarmed_roll: false,
            onset_saved: false,
            alarmed_log: false,
            alarmed_memory: false,
            blind_reported: false,
        }
    }

    /// Called from the 1.5 s tick in `cef_view::run`.
    ///
    /// One `stat` every time, one process-tree memory reading every twentieth, and a roll
    /// only when the log has actually passed its cap - which a healthy run never does. The
    /// tao thread owns this; nothing here is shared with CEF's threads.
    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);

        let len = std::fs::metadata(&self.log).map(|m| m.len()).unwrap_or(0);
        let grew = len.saturating_sub(self.last_len);
        self.last_len = len;

        // Sampled on the FIRST tick and every MEMORY_EVERY_TICKS after it, so a roll always
        // has a reading to report even if it happens immediately.
        if self.ticks % MEMORY_EVERY_TICKS == 1 {
            let sample = private_bytes();
            // A short figure silently raises the real ceiling, which is how a defence stops
            // working without anyone noticing. Say it once rather than let it pass.
            if !self.blind_reported {
                match &sample {
                    Some(m) if m.undercount => {
                        self.blind_reported = true;
                        eprintln!(
                            "[whatsapp-rs] watchdog: could not read part of the engine's                              process tree; the memory figure is an undercount and its ceiling                              is effectively higher than {} bytes",
                            self.private_alarm
                        );
                    }
                    None => {
                        self.blind_reported = true;
                        eprintln!(
                            "[whatsapp-rs] watchdog: the process-memory probe is unavailable;                              only the log-rate check is active this run"
                        );
                    }
                    _ => {}
                }
            }
            self.last_private = sample.map(|m| m.private);
            match self.last_private {
                Some(p) if p > self.private_alarm => self.memory_breaches += 1,
                _ => self.memory_breaches = 0,
            }
        }

        if grew >= FIREHOSE_TICK_BYTES {
            self.firehose_ticks += 1;
            if self.firehose_ticks >= FIREHOSE_TICKS && !self.alarmed_log {
                self.alarmed_log = true;
                let rate = grew as f64 / 1.5 / (1024.0 * 1024.0);
                alarm(&format!(
                    "The engine is in a fault loop: it is writing {rate:.1} MB/s to its log. \
                     The log is being capped, so the disk is safe. Quit and reopen WhatsApp \
                     if it stays slow."
                ));
            }
        } else {
            self.firehose_ticks = 0;
        }

        if len > self.cap {
            self.roll(len);
        }

        if self.memory_breaches >= MEMORY_BREACHES && !self.alarmed_memory {
            self.alarmed_memory = true;
            let gb = self.last_private.unwrap_or(0) as f64 / (1024.0 * 1024.0 * 1024.0);
            alarm(&format!(
                "WhatsApp is holding {gb:.1} GB of memory, which is far more than it should \
                 ever need. Quit and reopen it."
            ));
        }
    }

    /// Keep the onset if this is the first wrap, then empty the live log.
    ///
    /// **Truncate, never delete.** Chromium opens its log with `FILE_APPEND_DATA` and
    /// `FILE_SHARE_READ | FILE_SHARE_WRITE` and holds the handle for the life of the
    /// process (`base/logging.cc`, `InitializeLogFileHandle`). No `FILE_SHARE_DELETE`
    /// means `remove_file` fails with a sharing violation while the engine is up, but
    /// `SetEndOfFile` through a second write handle succeeds - and because an append-only
    /// handle carries no file position of its own, Chromium's next write simply lands at
    /// the new end of file. That is not a guess: during the incident the 243.6 GB file was
    /// truncated in place under the still-running process and refilled from zero.
    fn roll(&mut self, len: u64) {
        if !self.onset_saved {
            // Streamed rather than `fs::copy`, which uses `CopyFileExW` and opens the
            // source without sharing the writer's access. `File::open` shares read, write
            // and delete, so it can read a file the engine is appending to.
            let copied = (|| -> std::io::Result<()> {
                use std::io::Read as _;
                // `take(cap)`, so the pair of files really is bounded by twice the cap and
                // not by whatever one tick's overshoot happened to be. The onset is the
                // head of the log, which is the part that says what went wrong.
                let mut src = std::fs::File::open(&self.log)?.take(self.cap);
                let mut dst = std::fs::File::create(&self.onset)?;
                std::io::copy(&mut src, &mut dst)?;
                Ok(())
            })();
            if copied.is_ok() {
                self.onset_saved = true;
            }
        }
        // Opening and truncating are ONE outcome on purpose. An earlier version discarded
        // `set_len`'s result, so a successful open with a failed truncate reported a
        // successful roll, reset the failure counter, and left the log growing - a defence
        // whose own status line lies is worse than no defence.
        let rolled = std::fs::OpenOptions::new()
            .write(true)
            .open(&self.log)
            .and_then(|f| f.set_len(0));
        match rolled {
            Ok(()) => {
                self.last_len = 0;
                self.roll_failures = 0;
                // The private figure rides along deliberately: a roll is the one moment
                // this app is definitely misbehaving, so it is the moment worth knowing
                // how big the process tree had got. `tools/soak.ps1` also reads it back,
                // which is how the Toolhelp walk above is proved to return a real number
                // rather than quietly returning only this process.
                let mb = self
                    .last_private
                    .map(|p| (p / (1024 * 1024)).to_string())
                    .unwrap_or_else(|| "?".into());
                eprintln!(
                    "[whatsapp-rs] watchdog: rolled cef.log at {len} bytes, tree private {mb} MB"
                );
            }
            // The cap is the only thing standing between a firehose and the user's disk, so
            // if emptying the log stops working there is nothing left - and this module
            // exists because the last time that happened nobody was told. Not fatal, not
            // worth taking a messaging app down for, but not silent either.
            Err(e) => {
                self.roll_failures += 1;
                eprintln!(
                    "[whatsapp-rs] watchdog: could not roll cef.log ({} in a row): {e}",
                    self.roll_failures
                );
                if self.roll_failures >= ROLL_FAILURES && !self.alarmed_roll {
                    self.alarmed_roll = true;
                    alarm(
                        "WhatsApp cannot keep its engine's log file in check, so it is growing \
                         with no limit and will fill this drive. Quit WhatsApp from the tray, \
                         then delete cef.log from its data folder.",
                    );
                }
            }
        }
    }
}

/// One line to the redirected stderr the test scripts read, and one Windows toast.
///
/// Straight to `notify::toast` and not through `cef_view::raise_toast`, which is rate
/// limited because the page it serves is not trusted to be sane. Each caller fires at most
/// once per run and must not be dropped by a flood it is reporting.
fn alarm(message: &str) {
    eprintln!("[whatsapp-rs] watchdog: {message}");
    notify::toast("WhatsApp Rs", message);
}

/// A test can shrink either threshold without a rebuild; `tools/soak.ps1` uses both.
fn env_bytes(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.trim().parse::<u64>().ok()
}

/// What one memory sample saw, and whether it saw all of it.
///
/// The `undercount` flag exists because the two ways this walk can fail - the process
/// snapshot refusing, or a child refusing to open - both degrade it silently to "this
/// process only". That is the same shape as every other defect in this file's history: a
/// defence that stops working and says nothing. An undercount raises the effective ceiling
/// without moving the number that documents it, so it gets reported once per run.
struct TreeMemory {
    private: u64,
    undercount: bool,
}

/// Private commit for this process **and its children**.
///
/// This process alone would have been enough to catch the 2026-09-09 fault, because
/// `--in-process-gpu` had put the runaway GPU thread inside it. Removing that switch is
/// exactly what moves the next one out into a child, so measuring only ourselves would
/// have been a watchdog blinded by its own fix. CEF's render, GPU and utility processes
/// are all direct children of this one, so one level of `Toolhelp32` is the whole tree and
/// no recursion is needed.
///
/// Best effort throughout: a child that exits mid-walk, or that refuses to open, is
/// skipped rather than turned into a failure. An undercount raises no false alarm, and the
/// log-rate sensor is the one that catches a spinning process anyway - every CEF process
/// writes to the same `cef.log`.
#[cfg(target_os = "windows")]
fn private_bytes() -> Option<TreeMemory> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_VM_READ,
    };

    let mut total = unsafe { process_private(GetCurrentProcess()) }?;
    let mut undercount = false;
    let me = unsafe { GetCurrentProcessId() };

    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            // No walk at all: this is the browser process alone, which is roughly a tenth of
            // the tree and would quietly make the ceiling ten times harder to reach.
            return Some(TreeMemory {
                private: total,
                undercount: true,
            });
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ParentProcessID == me {
                    if let Ok(child) = OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                        false,
                        entry.th32ProcessID,
                    ) {
                        match process_private(child) {
                            Some(bytes) => total = total.saturating_add(bytes),
                            None => undercount = true,
                        }
                        let _ = CloseHandle(child);
                    } else {
                        // A child that exited between the snapshot and here is ordinary; one
                        // that refuses to open is not, and both look the same from here, so
                        // both are counted as short rather than assumed harmless.
                        undercount = true;
                    }
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    Some(TreeMemory {
        private: total,
        undercount,
    })
}

/// # Safety
/// `process` must be a valid process handle carrying at least
/// `PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ`.
#[cfg(target_os = "windows")]
unsafe fn process_private(process: windows::Win32::Foundation::HANDLE) -> Option<u64> {
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    unsafe {
        GetProcessMemoryInfo(
            process,
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            size,
        )
        .ok()?;
    }
    Some(counters.PrivateUsage as u64)
}

#[cfg(not(target_os = "windows"))]
fn private_bytes() -> Option<TreeMemory> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Never `PRIVATE_ALARM`: these tests run inside a cargo test binary whose own memory is
    /// nobody's business, and a toast from a unit test is a bug in the test.
    const NO_MEMORY_ALARM: u64 = u64::MAX;

    /// Small enough that a write of a few kilobytes is "past the cap", and far under
    /// `FIREHOSE_TICK_BYTES`, so none of these tests can trip the rate alarm either.
    const CAP: u64 = 4096;

    fn scratch() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "wa-rs-watchdog-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn under_the_cap_nothing_is_touched() {
        let dir = scratch();
        let mut w = Watchdog::armed(&dir, CAP, NO_MEMORY_ALARM);
        std::fs::write(log_path(&dir), vec![b'a'; CAP as usize - 1]).unwrap();

        w.tick();

        assert_eq!(
            std::fs::metadata(log_path(&dir)).unwrap().len(),
            CAP - 1,
            "a log inside its cap must be left alone"
        );
        assert!(!onset_path(&dir).exists(), "no onset without a roll");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn past_the_cap_rolls_and_keeps_the_head() {
        let dir = scratch();
        let mut w = Watchdog::armed(&dir, CAP, NO_MEMORY_ALARM);
        let mut body = vec![b'h'; CAP as usize];
        body.extend(vec![b't'; 500]);
        std::fs::write(log_path(&dir), &body).unwrap();

        w.tick();

        assert_eq!(
            std::fs::metadata(log_path(&dir)).unwrap().len(),
            0,
            "past the cap the live log is emptied"
        );
        let kept = std::fs::read(onset_path(&dir)).expect("onset written");
        assert_eq!(kept.len(), CAP as usize, "the onset is bounded by the cap");
        assert!(
            kept.iter().all(|b| *b == b'h'),
            "the onset is the HEAD of the log - where a root cause lives - not the tail"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_onset_is_the_first_wrap_and_never_overwritten() {
        let dir = scratch();
        let mut w = Watchdog::armed(&dir, CAP, NO_MEMORY_ALARM);

        std::fs::write(log_path(&dir), vec![b'1'; CAP as usize + 10]).unwrap();
        w.tick();
        std::fs::write(log_path(&dir), vec![b'2'; CAP as usize + 10]).unwrap();
        w.tick();

        assert_eq!(std::fs::metadata(log_path(&dir)).unwrap().len(), 0);
        let kept = std::fs::read(onset_path(&dir)).unwrap();
        assert!(
            kept.iter().all(|b| *b == b'1'),
            "a later wrap must not overwrite the onset; the tail is always in cef.log"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn arming_clears_the_log_but_never_the_onset() {
        let dir = scratch();
        std::fs::write(log_path(&dir), b"last run's log").unwrap();
        std::fs::write(onset_path(&dir), b"last run's fault").unwrap();

        let _w = Watchdog::armed(&dir, CAP, NO_MEMORY_ALARM);

        assert!(
            !log_path(&dir).exists(),
            "the live log belongs to the run that is starting"
        );
        assert_eq!(
            std::fs::read(onset_path(&dir)).expect("the onset must survive a restart"),
            b"last run's fault",
            "both alarms tell the user to quit and reopen; obeying that must not delete the \
             evidence of the fault they were told about"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_log_that_cannot_be_emptied_is_counted_not_ignored() {
        let dir = scratch();
        let mut w = Watchdog::armed(&dir, CAP, NO_MEMORY_ALARM);
        let log = log_path(&dir);
        std::fs::write(&log, vec![b'x'; CAP as usize + 10]).unwrap();
        // Read-only makes the write-open fail, which is the shape of every reason a roll
        // can fail: the cap stops working and the disk has nothing defending it.
        let mut perms = std::fs::metadata(&log).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&log, perms).unwrap();

        w.tick();

        assert_eq!(
            w.roll_failures, 1,
            "a failed roll must be counted, not swallowed"
        );
        assert!(!w.alarmed_roll, "one failure is not yet worth a toast");
        assert_eq!(
            std::fs::metadata(&log).unwrap().len(),
            CAP + 10,
            "nothing was emptied, and the code must not pretend otherwise"
        );

        // Clearing FILE_ATTRIBUTE_READONLY so the scratch directory can be deleted. Clippy
        // dislikes `set_readonly(false)` because on Unix it grants write to *everyone*; this
        // is a Windows-only test over a file in its own temp directory, and without it
        // `remove_dir_all` cannot remove what the test just made read-only.
        #[allow(clippy::permissions_set_readonly_false)]
        {
            let mut perms = std::fs::metadata(&log).unwrap().permissions();
            perms.set_readonly(false);
            std::fs::set_permissions(&log, perms).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_memory_probe_returns_a_real_number() {
        // Proves the Toolhelp/psapi FFI works at all. A silently failing probe would leave
        // the memory half of this watchdog permanently blind and permanently quiet.
        let seen = private_bytes();
        if cfg!(target_os = "windows") {
            let m = seen.expect("private_bytes must read this process on Windows");
            assert!(
                m.private > 0,
                "a running process holds more than zero private bytes"
            );
        }
    }

    #[test]
    fn one_high_memory_sample_is_not_enough_to_alarm() {
        let dir = scratch();
        // A ceiling of zero means every reading breaches it, so the only thing keeping this
        // quiet is the two-consecutive-samples rule - which is what guards against a pid
        // recycled between the process snapshot and the read.
        let mut w = Watchdog::armed(&dir, CAP, 0);

        w.tick();

        assert!(
            !w.alarmed_memory,
            "one sample over the ceiling must not raise a toast"
        );
        assert_eq!(
            w.memory_breaches,
            if cfg!(target_os = "windows") { 1 } else { 0 }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_log_is_not_an_error() {
        let dir = scratch();
        let mut w = Watchdog::armed(&dir, CAP, NO_MEMORY_ALARM);
        // CEF has not opened it yet; the first ticks happen before the engine is up.
        w.tick();
        assert!(!onset_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
