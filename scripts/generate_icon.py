#!/usr/bin/env python3
"""从 src-tauri/icons/icon.svg 生成多分辨率 icon.ico。

背景：手工拼装的 ico 曾出现 128 帧整帧透明、256 帧内容偏移被裁切的问题，
导致 exe 在桌面上以不同尺寸显示时图标偏移。这里改用 SVG 单源栅格化，
保证每一帧都居中且非空。

依赖（仅开发时使用，应用本身不需要）：
    pip install resvg-py pillow

用法：
    python scripts/generate_icon.py
"""
from __future__ import annotations

import io
import struct
import sys
from pathlib import Path

import resvg_py
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SVG_PATH = ROOT / "src-tauri" / "icons" / "icon.svg"
ICO_PATH = ROOT / "src-tauri" / "icons" / "icon.ico"
SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]


def main() -> int:
    svg = SVG_PATH.read_text(encoding="utf-8")
    # 先超采样到 1024，再交给 Pillow 生成各尺寸，边缘更平滑。
    png = resvg_py.svg_to_bytes(svg_string=svg, width=1024, height=1024)
    base = Image.open(io.BytesIO(png)).convert("RGBA")

    base.save(ICO_PATH, format="ICO", sizes=SIZES)

    # 自检：每帧必须非空且内容居中，避免再次产出坏帧。
    raw = ICO_PATH.read_bytes()
    count = struct.unpack("<H", raw[4:6])[0]
    for i in range(count):
        off = 6 + i * 16
        w, h, *_rest, size, offset = struct.unpack("<BBBBHHII", raw[off:off + 16])
        frame = Image.open(io.BytesIO(raw[offset:offset + size])).convert("RGBA")
        bbox = frame.split()[3].getbbox()
        if bbox is None:
            print(f"[icon] frame {w or 256}x{h or 256} is fully transparent", file=sys.stderr)
            return 1
        W, H = frame.size
        cx = (bbox[0] + bbox[2]) / 2
        cy = (bbox[1] + bbox[3]) / 2
        if abs(cx - W / 2) > 1.5 or abs(cy - H / 2) > 1.5:
            print(f"[icon] frame {W}x{H} off-center: bbox={bbox}", file=sys.stderr)
            return 1

    print(f"[icon] wrote {ICO_PATH} with {count} frames: {[s[0] for s in SIZES]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
