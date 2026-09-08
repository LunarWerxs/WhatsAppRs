//! The Chromium the executable carries, and where it unpacks to.
//!
//! v0.1.0 shipped as a folder: a 0.9 MB `whatsapp.exe` beside a 271 MB `libcef.dll` and
//! fifteen other files. #23 in DECISIONS.md is that it should be one file to download and
//! double-click. It cannot be one *static* binary - CEF exists only as that DLL, there is no
//! static Chromium - so instead the engine travels inside the exe as a compressed payload
//! appended after the PE image, and unpacks itself once into
//! `%LOCALAPPDATA%\WhatsAppRs\engine\<version>-<payload id>\`.
//!
//! Three things make this work, and each of them is load-bearing:
//!
//! 1. **`libcef.dll` is delay-loaded** (`/DELAYLOAD:libcef.dll`, added by `build.rs`).
//!    Without that, Windows resolves the import table before `main` runs and refuses to
//!    start an exe with no `libcef.dll` beside it - which is every copy of this exe until
//!    it has unpacked itself. `dumpbin -imports` says the exe imports twelve CEF symbols
//!    and all twelve are functions, which is the condition delay-load requires: a *data*
//!    import cannot be delayed.
//! 2. **[`prepare`] runs before anything that could reach CEF.** In `main` only the `--quit`
//!    branch comes first, and that one talks to a socket and exits without touching a CEF
//!    entry point - deliberately, so asking a running app to quit cannot start an unpack of
//!    its own. Everything else is behind this call, including `cef_view::intercept`, because
//!    CEF re-runs this executable for its renderer, GPU and utility processes and those
//!    processes must find the DLL too. They take the fast path here: compute the folder, load
//!    the DLLs, return. They never extract and never draw anything.
//! 3. **The engine folder is put on the DLL search path** with `SetDllDirectoryW` before
//!    `chrome_elf.dll` and `libcef.dll` are loaded from it by full path, in that order -
//!    `libcef.dll` has a load-time dependency on `chrome_elf.dll`, and ANGLE and the
//!    software-rendering fallback later ask for `libEGL.dll`, `libGLESv2.dll`,
//!    `vk_swiftshader.dll` and friends by bare name.
//!
//! What it does not change, and the README says so: the disk cost after first run is the
//! same ~350 MB, because it is the same engine, and the memory cost is identical.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Written by `tools/pack-payload.py` at the very end of the file. 64 bytes, little-endian.
const FOOTER_LEN: usize = 64;
const FOOTER_MAGIC: &[u8; 8] = b"WARSTRL1";
/// The first bytes of the *decompressed* stream, so a truncated or wrong-codec payload is
/// caught before any of it is believed.
const STREAM_MAGIC: &[u8; 8] = b"WARSPAY1";
/// The one codec. xz was measured against it; `tools/pack-payload.py --measure` has the
/// numbers and FINDINGS.md records the choice.
const CODEC_ZSTD: u32 = 1;
/// zstd was packed with a 128 MB window, which the decoder refuses by default.
const WINDOW_LOG_MAX: u32 = 27;

/// The manifest the extractor writes last. Its presence is what "this folder is finished"
/// means; a half-written folder has no manifest and is re-extracted.
const MANIFEST: &str = "engine.ok";

/// The two files that identify a usable engine folder, in dev builds as well as unpacked
/// ones: the DLL the exe delay-loads, and the resource pak CEF cannot start without.
const SENTINELS: [&str; 2] = ["libcef.dll", "resources.pak"];

/// Where the engine is and how it got there.
pub struct Engine {
    /// The folder holding `libcef.dll`, the `.pak` files and `locales/`.
    pub dir: PathBuf,
    /// True when this run unpacked the payload. Only ever true in the browser process.
    pub extracted: bool,
    /// True for a build that runs from `target/release`, with the engine beside the exe and
    /// no payload involved. `tools/build.ps1` produces one of these and the whole
    /// development loop depends on it still working.
    pub beside_exe: bool,
}

/// Must run before anything that could reach CEF: in `main`, before `cef_view::intercept`
/// and before every other branch except `--quit`, which touches no CEF entry point.
///
/// In the browser process this unpacks the engine if it is not already unpacked, showing a
/// small window while it does. In a CEF subprocess it only finds the folder and loads the
/// DLLs, which is a 64-byte read and two `LoadLibraryExW` calls.
pub fn prepare(is_subprocess: bool) -> Result<Engine, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe_dir = exe
        .parent()
        .ok_or_else(|| "the executable has no parent directory".to_string())?
        .to_path_buf();

    // A build from `target/release`: the cef crate has copied the runtime beside the exe, so
    // there is nothing to unpack and the payload (if this build even has one) is ignored.
    if SENTINELS.iter().all(|f| exe_dir.join(f).exists()) {
        load_dlls(&exe_dir)?;
        return Ok(Engine { dir: exe_dir, extracted: false, beside_exe: true });
    }

    let footer = Footer::read(&exe)?;
    let root = engine_root();
    let dir = root.join(format!("{}-{}", env!("CARGO_PKG_VERSION"), footer.id()));

    // Already unpacked, by an earlier run or by the browser process that spawned this one.
    if manifest_matches(&dir) {
        load_dlls(&dir)?;
        return Ok(Engine { dir, extracted: false, beside_exe: false });
    }

    if is_subprocess {
        // A subprocess reaching this is a bug, not a situation to recover from: the browser
        // process unpacks before it calls `cef_initialize`, which is what spawns children.
        // Say so rather than extracting 346 MB five times in parallel.
        return Err(format!(
            "a CEF subprocess found no engine at {} - the browser process should have unpacked it",
            dir.display()
        ));
    }

    unpack(&exe, &footer, &root, &dir)?;
    load_dlls(&dir)?;
    Ok(Engine { dir, extracted: true, beside_exe: false })
}

/// Old versions' engine folders, deleted after a new one has started successfully.
///
/// Not on the startup path: this walks and deletes ~350 MB of files and there is no reason
/// to make the user wait for it. Called from a background thread once the browser is up.
pub fn sweep_old(keep: &Path) {
    let Some(root) = keep.parent() else { return };
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep || !path.is_dir() {
            continue;
        }
        // Only ever folders this code wrote. The test is the manifest, not the name: a name
        // rule ("<something>-<16 hex>") would also match a `backup-0123456789abcdef` somebody
        // else left here, and `WHATSAPP_RS_ENGINE_DIR` deliberately lets this point at a
        // shared scratch directory. The staging names are ours by construction.
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let ours = name.starts_with(".tmp-") || name.starts_with(".old-") || wrote_manifest(&path);
        if ours {
            // The manifest goes first. If the rest of the delete fails half way - the most
            // likely reason being that another build of this app still has those DLLs mapped
            // - what is left behind has no manifest, so the next start treats it as unfinished
            // and re-extracts rather than loading a Chromium with holes in it.
            let _ = fs::remove_file(path.join(MANIFEST));
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Did this code write the folder at `path`? The manifest, and its first line, is the proof.
fn wrote_manifest(path: &Path) -> bool {
    fs::read_to_string(path.join(MANIFEST))
        .map(|text| text.starts_with("whatsapp-rs-engine 1"))
        .unwrap_or(false)
}

/// `%LOCALAPPDATA%\WhatsAppRs\engine`, or the same folder under `WHATSAPP_RS_DATA_DIR`.
///
/// It follows the data directory deliberately: the test harnesses point that at a scratch
/// profile, and an engine folder is exactly the kind of thing a test must not share with a
/// real, logged-in install.
fn engine_root() -> PathBuf {
    match std::env::var_os("WHATSAPP_RS_ENGINE_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => crate::paths::data_dir().join("engine"),
    }
}

// ---------------------------------------------------------------------------------------
// The payload
// ---------------------------------------------------------------------------------------

struct Footer {
    payload_len: u64,
    stream_len: u64,
    hash: [u8; 32],
    codec: u32,
}

impl Footer {
    /// The last 64 bytes of our own executable. Reading it is a seek and a 64-byte read, so
    /// every CEF subprocess can afford to do it rather than be told the answer.
    fn read(exe: &Path) -> Result<Footer, String> {
        let mut file = File::open(exe).map_err(|e| format!("cannot read {}: {e}", exe.display()))?;
        let len = file
            .seek(SeekFrom::End(0))
            .map_err(|e| format!("seek: {e}"))?;
        if len < FOOTER_LEN as u64 {
            return Err("this executable carries no engine".into());
        }
        file.seek(SeekFrom::End(-(FOOTER_LEN as i64)))
            .map_err(|e| format!("seek: {e}"))?;
        let mut buf = [0u8; FOOTER_LEN];
        file.read_exact(&mut buf).map_err(|e| format!("read: {e}"))?;
        if &buf[56..64] != FOOTER_MAGIC {
            return Err(
                "this executable carries no engine payload - build it with tools/build.ps1 \
                 -Single, or run it from a folder that has libcef.dll in it"
                    .into(),
            );
        }
        let footer = Footer {
            payload_len: u64::from_le_bytes(buf[0..8].try_into().unwrap()),
            stream_len: u64::from_le_bytes(buf[8..16].try_into().unwrap()),
            hash: buf[16..48].try_into().unwrap(),
            codec: u32::from_le_bytes(buf[48..52].try_into().unwrap()),
        };
        if footer.codec != CODEC_ZSTD {
            return Err(format!("unknown payload codec {}", footer.codec));
        }
        // Checked, not plain arithmetic. This is the last 64 bytes of a file strangers download,
        // so `payload_len` is whatever is on their disk: `u64::MAX - 63` would wrap a plain add
        // to zero, sail past this guard, and surface later as an unexplained zstd error instead
        // of the sentence below.
        let claimed = footer
            .payload_len
            .checked_add(FOOTER_LEN as u64)
            .filter(|&total| total <= len && footer.payload_len <= i64::MAX as u64);
        if claimed.is_none() {
            return Err("the engine payload is truncated".into());
        }
        Ok(footer)
    }

    /// The folder name's second half: the first eight bytes of the payload's SHA-256, so a
    /// rebuilt 0.2.0 with a different engine unpacks beside the old one instead of being
    /// mistaken for it.
    fn id(&self) -> String {
        self.hash[..8].iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// One file in the payload.
struct Entry {
    path: String,
    size: u64,
    crc: u32,
}

/// Reads the payload out of the exe, decompressed, as a stream.
fn payload_reader(exe: &Path, footer: &Footer) -> Result<impl Read, String> {
    let mut file = File::open(exe).map_err(|e| format!("cannot read {}: {e}", exe.display()))?;
    let start = file
        .seek(SeekFrom::End(-(FOOTER_LEN as i64 + footer.payload_len as i64)))
        .map_err(|e| format!("seek: {e}"))?;
    let _ = start;
    let compressed = file.take(footer.payload_len);
    let mut decoder = zstd::stream::read::Decoder::new(io::BufReader::new(compressed))
        .map_err(|e| format!("zstd: {e}"))?;
    decoder
        .window_log_max(WINDOW_LOG_MAX)
        .map_err(|e| format!("zstd window: {e}"))?;
    Ok(decoder)
}

fn read_exact_n(r: &mut impl Read, n: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).map_err(|e| format!("payload: {e}"))?;
    Ok(buf)
}

/// Returns the index, and how many bytes of the stream it occupied - which is what makes the
/// footer's `stream_len` checkable against the sizes the index itself claims.
fn read_index(r: &mut impl Read) -> Result<(Vec<Entry>, u64), String> {
    let magic = read_exact_n(r, 8)?;
    if magic != STREAM_MAGIC {
        return Err("the engine payload does not start with its own magic".into());
    }
    let count = u32::from_le_bytes(read_exact_n(r, 4)?.try_into().unwrap());
    if count == 0 || count > 4096 {
        return Err(format!("the engine payload claims {count} files"));
    }
    let mut consumed: u64 = 12;
    let mut entries = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let name_len = u16::from_le_bytes(read_exact_n(r, 2)?.try_into().unwrap()) as usize;
        let name = String::from_utf8(read_exact_n(r, name_len)?)
            .map_err(|_| "a payload path is not UTF-8".to_string())?;
        let rest = read_exact_n(r, 12)?;
        entries.push(Entry {
            path: name,
            size: u64::from_le_bytes(rest[0..8].try_into().unwrap()),
            crc: u32::from_le_bytes(rest[8..12].try_into().unwrap()),
        });
        consumed += 14 + name_len as u64;
    }
    for e in &entries {
        safe_relative(&e.path)?;
    }
    Ok((entries, consumed))
}

/// A path out of the payload is only ever used to create a file under the engine folder, so
/// it must be relative, have no `..` and no drive letter. The payload is ours and this is
/// belt and braces, but it is four lines and the alternative is a zip-slip.
fn safe_relative(path: &str) -> Result<PathBuf, String> {
    let bad = path.is_empty()
        || path.starts_with('/')
        || path.contains(':')
        || path.contains('\\')
        || path.split('/').any(|c| c.is_empty() || c == "." || c == "..");
    if bad {
        return Err(format!("refusing payload path {path:?}"));
    }
    Ok(PathBuf::from(path.replace('/', std::path::MAIN_SEPARATOR_STR)))
}

// ---------------------------------------------------------------------------------------
// Unpacking
// ---------------------------------------------------------------------------------------

/// Unpack into a staging folder, verify every file, then move it into place.
///
/// Never writes into `dir` directly. A run that dies half way through leaves a `.tmp-<pid>`
/// folder that the next start sweeps, rather than a folder that looks finished and is not -
/// which would be a corrupt Chromium and a crash with no explanation.
fn unpack(exe: &Path, footer: &Footer, root: &Path, dir: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| format!("cannot create {}: {e}", root.display()))?;
    let staging = root.join(format!(".tmp-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| format!("cannot create {}: {e}", staging.display()))?;

    // The index is read here, not inside the extractor, so the progress bar's total is the sum
    // of the file sizes and not `stream_len` - the difference being the index itself, which the
    // extractor consumes without counting, so a bar scaled to `stream_len` would stop a few
    // hundred bytes short of full every single time.
    let mut reader = payload_reader(exe, footer)?;
    let (entries, index_bytes) = read_index(&mut reader)?;
    let total: u64 = entries.iter().map(|e| e.size).sum();

    // The index and the footer have to agree before 346 MB of disk is spent on them. They are
    // written by the same script from the same bytes, so a disagreement means the payload is
    // not the one that was packed.
    let claimed = index_bytes.checked_add(total);
    if claimed != Some(footer.stream_len) {
        return Err(format!(
            "the engine payload's index adds up to {claimed:?} bytes, but its footer says {}",
            footer.stream_len
        ));
    }

    let done = Arc::new(AtomicU64::new(0));
    let window = crate::setup_window::Window::show(done.clone(), total);

    let result = extract_all(&mut reader, &entries, &staging, &done);
    window.close();
    result.map_err(|e| {
        let _ = fs::remove_dir_all(&staging);
        e
    })?;

    // Two double-clicks in a row start two processes, and both reach this point before either
    // takes the single-instance lock - that lock lives on the other side of `cef_initialize`,
    // which cannot be called until the engine exists. So the loser of that race must not fail:
    // if the folder is there and good now, whoever wrote it did the same work, and adopting it
    // is the right answer.
    let adopt = |staging: &Path| {
        let _ = fs::remove_dir_all(staging);
        Ok(())
    };
    if manifest_matches(dir) {
        return adopt(&staging);
    }

    // Replace whatever is at `dir` - a half-written folder from an interrupted run, or an
    // older extraction of the same payload that failed verification. Windows will not rename
    // onto an existing directory, so the old one moves aside first.
    if dir.exists() {
        let aside = root.join(format!(".old-{}", std::process::id()));
        let _ = fs::remove_dir_all(&aside);
        if let Err(err) = fs::rename(dir, &aside) {
            // The usual reason is that another instance already has those DLLs mapped, which
            // means the folder it is running from is a finished one.
            if manifest_matches(dir) {
                return adopt(&staging);
            }
            let _ = fs::remove_dir_all(&staging);
            return Err(format!("cannot move the old engine aside: {err}"));
        }
        let _ = fs::remove_dir_all(&aside);
    }
    match fs::rename(&staging, dir) {
        Ok(()) => Ok(()),
        Err(_) if manifest_matches(dir) => adopt(&staging),
        Err(err) => {
            let _ = fs::remove_dir_all(&staging);
            Err(format!("cannot move the engine into {}: {err}", dir.display()))
        }
    }
}

fn extract_all(
    reader: &mut impl Read,
    entries: &[Entry],
    staging: &Path,
    done: &AtomicU64,
) -> Result<(), String> {
    let mut manifest = String::from("whatsapp-rs-engine 1\n");
    let mut buf = vec![0u8; 1 << 20];
    for entry in entries {
        let rel = safe_relative(&entry.path)?;
        let out_path = staging.join(&rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let mut out = io::BufWriter::new(
            File::create(&out_path)
                .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?,
        );
        let mut left = entry.size;
        let mut crc = Crc32::new();
        while left > 0 {
            let want = left.min(buf.len() as u64) as usize;
            reader
                .read_exact(&mut buf[..want])
                .map_err(|e| format!("payload ended early in {}: {e}", entry.path))?;
            crc.update(&buf[..want]);
            out.write_all(&buf[..want])
                .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
            left -= want as u64;
            done.fetch_add(want as u64, Ordering::Relaxed);
        }
        out.flush()
            .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
        drop(out);

        // Verify before the folder is ever trusted: the size Windows reports and the CRC of
        // what was actually written, against what the payload said it packed.
        let crc = crc.finish();
        if crc != entry.crc {
            return Err(format!(
                "{} unpacked with checksum {crc:08x}, expected {:08x}",
                entry.path, entry.crc
            ));
        }
        let written = fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
        if written != entry.size {
            return Err(format!(
                "{} unpacked as {written} bytes, expected {}",
                entry.path, entry.size
            ));
        }
        manifest.push_str(&format!("{} {crc:08x} {}\n", entry.size, entry.path));
    }

    // Last, so its presence means the whole folder is good.
    fs::write(staging.join(MANIFEST), manifest)
        .map_err(|e| format!("cannot write the engine manifest: {e}"))
}

/// Is the folder at `dir` a finished extraction whose files are all still the right size?
///
/// Sizes only, not checksums: this runs on every start including every CEF subprocess, and
/// re-reading 346 MB five times per launch to prove nothing changed would be a real cost for
/// a case that the extractor already checked byte by byte.
fn manifest_matches(dir: &Path) -> bool {
    let Ok(manifest) = fs::read_to_string(dir.join(MANIFEST)) else {
        return false;
    };
    let mut lines = manifest.lines();
    if lines.next() != Some("whatsapp-rs-engine 1") {
        return false;
    }
    let mut count = 0;
    for line in lines {
        let mut parts = line.splitn(3, ' ');
        let (Some(size), Some(_crc), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            return false;
        };
        let Ok(size) = size.parse::<u64>() else {
            return false;
        };
        let Ok(rel) = safe_relative(path) else {
            return false;
        };
        match fs::metadata(dir.join(rel)) {
            Ok(m) if m.len() == size => count += 1,
            _ => return false,
        }
    }
    count > 0
}

/// CRC-32 (the zlib polynomial), so the Rust side checks exactly what the Python packer
/// wrote. Table-driven, built once per file; a whole 285 MB DLL costs a few hundred
/// milliseconds, which is inside the noise of writing it to disk.
struct Crc32 {
    state: u32,
}

impl Crc32 {
    fn new() -> Crc32 {
        Crc32 { state: 0xFFFF_FFFF }
    }

    fn update(&mut self, data: &[u8]) {
        let mut state = self.state;
        for &byte in data {
            let mut c = (state ^ byte as u32) & 0xFF;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            state = c ^ (state >> 8);
        }
        self.state = state;
    }

    fn finish(self) -> u32 {
        !self.state
    }
}

// ---------------------------------------------------------------------------------------
// Loading the DLLs
// ---------------------------------------------------------------------------------------

/// Put the engine folder on the DLL search path and load the two DLLs that must be found by
/// full path, in the order CEF requires.
///
/// `chrome_elf.dll` first: `libcef.dll` names it as a load-time dependency (`dumpbin
/// -dependents`), and loading `libcef.dll` by full path with `LOAD_WITH_ALTERED_SEARCH_PATH`
/// would find it anyway - loading it explicitly first makes the order a statement rather
/// than a side effect.
///
/// Then `libcef.dll` itself, which is what makes the delay-load stub resolve. The delay-load
/// helper calls `LoadLibraryA("libcef.dll")` with the bare name on the first CEF call; the
/// loader answers that with the module of the same base name already in the process, which
/// is the one loaded here, from the engine folder.
///
/// `SetDllDirectoryW` covers everything after that: ANGLE asks for `libEGL.dll` and
/// `libGLESv2.dll` by name, and the software-rendering fallback for `vk_swiftshader.dll`,
/// `vulkan-1.dll`, `dxcompiler.dll`, `dxil.dll` and `d3dcompiler_47.dll`. It also removes
/// the current working directory from the search order, which is a small bonus.
#[cfg(target_os = "windows")]
fn load_dlls(dir: &Path) -> Result<(), String> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::System::LibraryLoader::{
        LoadLibraryExW, SetDllDirectoryW, LOAD_WITH_ALTERED_SEARCH_PATH,
    };

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(once(0)).collect()
    }

    let dir_w = wide(dir);
    unsafe {
        SetDllDirectoryW(PCWSTR(dir_w.as_ptr()))
            .map_err(|e| format!("SetDllDirectory({}): {e}", dir.display()))?;
        for name in ["chrome_elf.dll", "libcef.dll"] {
            let path = dir.join(name);
            let path_w = wide(&path);
            LoadLibraryExW(PCWSTR(path_w.as_ptr()), None, LOAD_WITH_ALTERED_SEARCH_PATH)
                .map_err(|e| format!("cannot load {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn load_dlls(_dir: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CRC has to agree with Python's `zlib.crc32`, which is what wrote the numbers in
    /// the payload. These three are the values `zlib.crc32` returns.
    #[test]
    fn crc32_matches_zlib() {
        for (input, expected) in [
            ("", 0x0000_0000u32),
            ("a", 0xE8B7_BE43),
            ("123456789", 0xCBF4_3926),
        ] {
            let mut crc = Crc32::new();
            crc.update(input.as_bytes());
            assert_eq!(crc.finish(), expected, "crc32({input:?})");
        }
    }

    #[test]
    fn crc32_is_the_same_split_across_chunks() {
        let data: Vec<u8> = (0..=255u8).cycle().take(10_000).collect();
        let mut whole = Crc32::new();
        whole.update(&data);
        let mut split = Crc32::new();
        for chunk in data.chunks(97) {
            split.update(chunk);
        }
        assert_eq!(whole.finish(), split.finish());
    }

    #[test]
    fn payload_paths_cannot_escape() {
        for bad in ["", "/abs", "..", "a/../b", "C:/x", "a\\b", "a//b", "./a"] {
            assert!(safe_relative(bad).is_err(), "{bad:?} should be refused");
        }
        assert_eq!(safe_relative("locales/en-US.pak").unwrap(), Path::new("locales").join("en-US.pak"));
    }
}
