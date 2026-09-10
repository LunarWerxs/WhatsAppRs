# Building Servo on this Windows box

Every one of these was hit in order on 2026-09-06 and cost a build cycle each. Do them all up
front. The clone lives at `D:\.DevScratch\forks\servo` (main, shallow 200; moved there
2026-09-08 from `D:\NEWProjects\servo` - third-party clones live under `.DevScratch\forks`).

## Prerequisites that were missing or off PATH

1. **`uv`** is mandatory. `mach.bat` is literally `uv run --frozen python mach %*`, so running
   `python mach ...` directly skips the venv and dies on `No module named 'mozlog'`.
   Install user-level: `python -m pip install --user uv`, and put the user scripts dir on PATH
   (`python -c "import sysconfig; print(sysconfig.get_path('scripts', 'nt_user'))"`).
2. **GStreamer is not installed.** Build with `--media-stack dummy`; WhatsApp messaging needs no
   audio or video pipeline. Without the flag `mach` refuses with "GStreamer libraries not found".
3. **LLVM 22 is installed at `C:\Program Files\LLVM\bin` but not on PATH** (winget does not add
   it; Servo's own `python/servo/platform/windows.py` says so). `mach` sets `CC=clang-cl.exe` and
   the cargo config sets `linker = lld-link.exe`; both live in that directory. Prepend it to PATH
   and set `LIBCLANG_PATH` to it for bindgen.
4. Do NOT use `~/.rustup/toolchains/<ver>/lib/rustlib/x86_64-pc-windows-msvc/bin/gcc-ld/lld-link.exe`
   as the linker. It is Rust's self-contained shim: it prepends its own `-flavor link` before
   calling `rust-lld`, rustc also passes `-flavor link`, and the second one is read as a file
   named `link`. Use LLVM's real `lld-link.exe`.
5. The pinned toolchain is `1.97.1` (`rust-toolchain.toml`); rustup installs it on first use.

## The command

```powershell
$env:PATH = "C:\Program Files\LLVM\bin;" + (python -c "import sysconfig; print(sysconfig.get_path('scripts', 'nt_user'))") + ";" + $env:PATH
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
Set-Location D:\.DevScratch\forks\servo
uv run --frozen python mach build --release --media-stack dummy
```

Run it under the fair-CPU job wrapper on this shared box:
`pwsh -File ~/.claude/tools/fairjob.ps1 -Weight 3 -Run "uv run --frozen python mach build --release --media-stack dummy"`.

## Running the Cache Storage web-platform-tests

Servo's CI skips the whole directory: `tests/wpt/include.ini` has `[service-workers] skip: true`.
Passing the path explicitly is the intended way to run it locally:

```powershell
uv run --frozen python mach test-wpt --release tests/wpt/tests/service-workers/cache-storage/ --log-raw wpt-cache.log
```

Then regenerate the expectation files under `tests/wpt/meta/service-workers/cache-storage/` from
that log with `mach update-wpt` (see the recipe confirmed in this session's notes), and check the
diff: subtests that flipped FAIL to PASS are the proof, not the summary line.

## Running the engine unit tests without the full build

`cargo test -p storage --lib cache_storage` compiles only the storage crate. It contends with a
running `mach build` for the target-dir lock, so do not run both at once.
