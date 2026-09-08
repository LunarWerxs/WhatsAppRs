#!/usr/bin/env python3
"""Turn the bundle folder plus whatsapp.exe into the one file v0.2.0 ships.

The engine cannot be linked into the executable - CEF exists only as a 271 MB DLL - so it
travels behind it: the exe is written out unchanged, the compressed engine is appended after
the end of the PE image (which Windows ignores), and a 64-byte footer at the very end tells
the running exe where its own payload starts. `src/engine.rs` is the other half.

    python tools/pack-payload.py D:/wa-bundle/whatsapp --out D:/wa-bundle/whatsapp.exe
    python tools/pack-payload.py D:/wa-bundle/whatsapp --measure     # compare codecs, write nothing
    python tools/pack-payload.py --verify D:/wa-bundle/whatsapp-rs-0.2.0-windows-x64.exe

Measured 2026-09-08 on this bundle (362,736,869 bytes of engine):

    codec      size MB   ratio   pack s   unpack s
    zstd:19      131.2    2.76     41.6       0.51
    zstd:22      128.4    2.82    167.8       0.50
    xz:6         126.2    2.87    107.9       3.65
    xz:9         122.1    2.97    153.2       3.64

zstd:22 is the default because 6 MB of download is worth less than three seconds of every
stranger's first run. `--codec xz:9` still works if that trade ever changes; the Rust side
would need the matching decoder.

Stream layout, little-endian, before compression:

    magic   b"WARSPAY1"
    u32     file count
    per file, sorted by path:
        u16 path length, path (UTF-8, '/' separated, relative)
        u64 uncompressed size
        u32 CRC-32 of the contents
    then every file's contents, concatenated in the same order.

Footer, the last 64 bytes of the exe:

    u64      compressed payload length
    u64      uncompressed stream length
    [32]     SHA-256 of the compressed payload
    u32      codec (1 = zstd)
    u32      format version (1)
    b"WARSTRL1"

`whatsapp.exe` itself is excluded from the payload: the exe carrying it IS that file.
"""

from __future__ import annotations

import argparse
import hashlib
import struct
import sys
import time
import zlib
from pathlib import Path

MAGIC = b"WARSPAY1"
FOOTER_MAGIC = b"WARSTRL1"
FOOTER_LEN = 64
CODECS = {"zstd": 1, "xz": 2}
# Excluded from the payload: the exe that carries the payload.
SKIP = {"whatsapp.exe"}


def collect(root: Path) -> list[tuple[str, Path]]:
    files = []
    for p in sorted(root.rglob("*")):
        if not p.is_file():
            continue
        rel = p.relative_to(root).as_posix()
        if rel in SKIP:
            continue
        files.append((rel, p))
    if not files:
        sys.exit(f"no files under {root}")
    return files


def build_stream(root: Path) -> bytes:
    files = collect(root)
    header = bytearray(MAGIC)
    header += struct.pack("<I", len(files))
    blobs = []
    for rel, path in files:
        data = path.read_bytes()
        name = rel.encode("utf-8")
        header += struct.pack("<H", len(name)) + name
        header += struct.pack("<QI", len(data), zlib.crc32(data) & 0xFFFFFFFF)
        blobs.append(data)
    return bytes(header) + b"".join(blobs)


def compress(stream: bytes, codec: str) -> tuple[bytes, float]:
    name, _, level = codec.partition(":")
    level = int(level) if level else 0
    t0 = time.perf_counter()
    if name == "xz":
        import lzma

        filt = [{"id": lzma.FILTER_LZMA2, "preset": level | lzma.PRESET_EXTREME}]
        out = lzma.compress(stream, format=lzma.FORMAT_XZ, check=lzma.CHECK_NONE, filters=filt)
    elif name == "zstd":
        import zstandard

        # window_log 27 (128 MB) so the compressor can match across the whole 346 MB stream;
        # `src/engine.rs` raises the decoder's limit to the same number. threads=-1 uses every
        # core, which is the difference between three minutes and twenty.
        params = zstandard.ZstdCompressionParameters.from_level(
            level, window_log=27, threads=-1, write_checksum=1
        )
        out = zstandard.ZstdCompressor(compression_params=params).compress(stream)
    else:
        sys.exit(f"unknown codec {codec}")
    return out, time.perf_counter() - t0


def decompress_time(blob: bytes, codec: str, expected: int) -> float:
    name = codec.split(":", 1)[0]
    t0 = time.perf_counter()
    if name == "xz":
        import lzma

        out = lzma.decompress(blob, format=lzma.FORMAT_XZ)
    else:
        import zstandard

        out = zstandard.ZstdDecompressor(max_window_size=1 << 27).decompress(
            blob, max_output_size=expected
        )
    dt = time.perf_counter() - t0
    if len(out) != expected:
        sys.exit(f"{codec} round trip lost bytes: {len(out)} != {expected}")
    return dt


def verify(exe: Path) -> None:
    """Read a packed exe back the way `src/engine.rs` does, and check every file.

    Independent of the writer above on purpose: it parses the footer from the end of the
    file, seeks to the payload, and walks the index, so a format the Rust reader would choke
    on is caught here instead of in a message box on a stranger's machine.
    """
    raw = exe.read_bytes()
    footer = raw[-FOOTER_LEN:]
    (payload_len, stream_len, digest, codec, version, magic) = struct.unpack("<QQ32sII8s", footer)
    if magic != FOOTER_MAGIC:
        sys.exit(f"{exe} has no payload footer")
    blob = raw[-(FOOTER_LEN + payload_len) : -FOOTER_LEN]
    name = {v: k for k, v in CODECS.items()}.get(codec)
    if name is None:
        sys.exit(f"unknown codec id {codec} in the footer")
    print(f"footer : codec {name} v{version}  payload {payload_len:,}  stream {stream_len:,}")
    if hashlib.sha256(blob).digest() != digest:
        sys.exit("the payload does not match the SHA-256 in the footer")

    # Honour the codec the footer names. Hard-coding zstd here would have made --verify a lie
    # for anything packed with --codec xz:9, which is a documented option.
    if name == "xz":
        import lzma

        stream = lzma.decompress(blob, format=lzma.FORMAT_XZ)
    else:
        import zstandard

        stream = zstandard.ZstdDecompressor(max_window_size=1 << 27).decompress(
            blob, max_output_size=stream_len
        )
    if len(stream) != stream_len:
        sys.exit(f"decompressed {len(stream)} bytes, footer said {stream_len}")
    if stream[:8] != MAGIC:
        sys.exit("the stream does not start with its magic")

    count = struct.unpack_from("<I", stream, 8)[0]
    at = 12
    index = []
    for _ in range(count):
        (name_len,) = struct.unpack_from("<H", stream, at)
        at += 2
        name = stream[at : at + name_len].decode("utf-8")
        at += name_len
        size, crc = struct.unpack_from("<QI", stream, at)
        at += 12
        index.append((name, size, crc))
    for name, size, crc in index:
        got = zlib.crc32(stream[at : at + size]) & 0xFFFFFFFF
        status = "ok" if got == crc else f"BAD crc {got:08x} != {crc:08x}"
        print(f"  {size:>12,}  {crc:08x}  {name}  {status}")
        if got != crc:
            sys.exit(1)
        at += size
    if at != stream_len:
        sys.exit(f"{stream_len - at} bytes left over after the last file")
    print(f"verify : {count} files, every checksum matches, nothing left over")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("bundle", type=Path, nargs="?", help="the folder tools/bundle.ps1 produced")
    ap.add_argument("--out", type=Path, help="where to write the single executable")
    ap.add_argument("--exe", type=Path, help="the stub exe (default: <bundle>/whatsapp.exe)")
    ap.add_argument("--codec", default="zstd:22")
    ap.add_argument("--measure", action="store_true", help="compare codecs, write nothing")
    ap.add_argument("--verify", type=Path, help="read a packed exe back and check every file")
    args = ap.parse_args()

    if args.verify:
        verify(args.verify)
        return
    if not args.bundle:
        sys.exit("a bundle folder is required unless --verify")

    stream = build_stream(args.bundle)
    print(f"engine : {len(stream):,} bytes in {len(collect(args.bundle))} files", flush=True)

    if args.measure:
        print(f"{'codec':<10} {'size MB':>9} {'ratio':>7} {'pack s':>8} {'unpack s':>9}", flush=True)
        for codec in ("zstd:19", "zstd:22", "xz:6", "xz:9"):
            blob, ct = compress(stream, codec)
            dt = decompress_time(blob, codec, len(stream))
            print(
                f"{codec:<10} {len(blob)/1e6:>9.1f} {len(stream)/len(blob):>7.2f}"
                f" {ct:>8.1f} {dt:>9.2f}",
                flush=True,
            )
        return

    if not args.out:
        sys.exit("--out is required unless --measure")
    exe = args.exe or args.bundle / "whatsapp.exe"
    if not exe.is_file():
        sys.exit(f"no stub executable at {exe}")
    stub = exe.read_bytes()
    if stub[-len(FOOTER_MAGIC):] == FOOTER_MAGIC:
        sys.exit(f"{exe} already carries a payload - point --exe at the plain build")

    blob, seconds = compress(stream, args.codec)
    footer = struct.pack(
        "<QQ32sII8s",
        len(blob),
        len(stream),
        hashlib.sha256(blob).digest(),
        CODECS[args.codec.split(":", 1)[0]],
        1,
        FOOTER_MAGIC,
    )
    assert len(footer) == FOOTER_LEN, len(footer)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_bytes(stub + blob + footer)
    written = args.out.stat().st_size
    print(
        f"payload: {len(blob):,} bytes  {args.codec}  packed in {seconds:.0f}s"
        f"  ({len(stream)/len(blob):.2f}x)"
    )
    print(f"exe    : {args.out}  {written:,} bytes  ({written/1e6:.1f} MB)")
    print(f"sha256 : {hashlib.sha256(args.out.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
