#!/usr/bin/env python3
"""从 SG2002 串口捕获中提取 ARCEOS_FILE 块并保存到本地目录。"""

from __future__ import annotations

import argparse
import base64
import re
import sys
from pathlib import Path

BEGIN_RE = re.compile(
    rb"=== ARCEOS_FILE_BEGIN (\S+) (?:(\d+) bin|b64text) ===\r?\n",
)
END_RE = re.compile(rb"=== ARCEOS_FILE_END (\S+) ===")


def sanitize_b64(data: bytes) -> bytes:
    return bytes(
        ch
        for ch in data
        if ch in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/="
    )


def extract_files(capture: bytes) -> list[tuple[str, str, int | None, bytes]]:
    out: list[tuple[str, str, int | None, bytes]] = []
    pos = 0
    while True:
        m = BEGIN_RE.search(capture, pos)
        if not m:
            break
        name = m.group(1).decode("ascii")
        size_s = m.group(2)
        kind = "bin" if size_s else "b64text"
        expected = int(size_s) if size_s else None
        body_start = m.end()
        end_pat = f"=== ARCEOS_FILE_END {name} ===".encode("ascii")
        end_idx = capture.find(end_pat, body_start)
        if end_idx < 0:
            print(f"warn: missing END marker for {name}", file=sys.stderr)
            break
        body = capture[body_start:end_idx]
        body = sanitize_b64(body.strip(b"\r\n"))
        out.append((name, kind, expected, body))
        pos = end_idx + len(end_pat)
    return out


def save_files(capture_path: Path, out_dir: Path) -> int:
    data = capture_path.read_bytes()
    items = extract_files(data)
    if not items:
        print(f"error: no ARCEOS_FILE blocks in {capture_path}", file=sys.stderr)
        return 1

    out_dir.mkdir(parents=True, exist_ok=True)
    saved = 0
    for name, kind, expected, body in items:
        dst = out_dir / name
        if kind == "b64text":
            dst.write_bytes(body)
            print(f"saved {dst} ({len(body)} bytes base64 text)")
            saved += 1
            continue

        try:
            raw = base64.b64decode(body, validate=False)
        except Exception as e:
            print(f"error: decode {name}: {e}", file=sys.stderr)
            continue
        if expected is not None and len(raw) != expected:
            print(
                f"warn: {name} size {len(raw)} != expected {expected}",
                file=sys.stderr,
            )
        dst.write_bytes(raw)
        print(f"saved {dst} ({len(raw)} bytes)")
        saved += 1
    print(f"done: {saved} file(s) -> {out_dir.resolve()}")
    return 0 if saved else 1


def main() -> int:
    p = argparse.ArgumentParser(description="Extract ARCEOS serial file exports")
    p.add_argument("capture", type=Path, help="serial capture file (binary ok)")
    p.add_argument(
        "out_dir",
        type=Path,
        nargs="?",
        default=Path("."),
        help="output directory (default: current directory)",
    )
    args = p.parse_args()
    if not args.capture.is_file():
        print(f"error: not found: {args.capture}", file=sys.stderr)
        return 1
    return save_files(args.capture, args.out_dir)


if __name__ == "__main__":
    raise SystemExit(main())
