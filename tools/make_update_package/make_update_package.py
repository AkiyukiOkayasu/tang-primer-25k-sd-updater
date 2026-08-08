#!/usr/bin/env python3
"""update package (FPGAOSC.UPD) を生成するホスト側ツール。

spec には各プロジェクトの `update_spec.conf` のパスを渡す。
"""

from __future__ import annotations

import argparse
import hashlib
import struct
import sys
import zlib
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from update_spec import UpdateSpec, load_update_spec


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Create update package")
    parser.add_argument("payload", type=Path, help="app binary bitstream")
    parser.add_argument("output", type=Path, help="output update package")
    parser.add_argument("--spec", type=Path, required=True, help="update_spec.conf path")
    parser.add_argument(
        "--app-version",
        type=lambda value: int(value, 0),
        default=0,
        help="application version encoded as u32",
    )
    return parser.parse_args()


def build_header(
    payload: bytes,
    *,
    spec: UpdateSpec,
    app_version: int,
) -> bytes:
    if len(payload) == 0:
        raise ValueError("payload is empty")
    if len(payload) > spec.app_size:
        raise ValueError(f"payload is too large for app slot: {len(payload)} > {spec.app_size}")

    payload_crc32 = zlib.crc32(payload) & 0xFFFF_FFFF
    payload_sha256 = hashlib.sha256(payload).digest()

    header = bytearray(spec.header_size)
    header[0:8] = spec.magic
    struct.pack_into("<I", header, 0x08, spec.format_version)
    struct.pack_into("<I", header, 0x0C, spec.target_hw_id)
    struct.pack_into("<I", header, 0x10, spec.target_fpga_id)
    struct.pack_into("<I", header, 0x14, spec.flash_layout_id)
    struct.pack_into("<I", header, 0x18, app_version)
    struct.pack_into("<I", header, 0x1C, spec.header_size)
    struct.pack_into("<I", header, 0x20, len(payload))
    struct.pack_into("<I", header, 0x24, payload_crc32)
    header[0x28:0x48] = payload_sha256
    return bytes(header)


def main() -> None:
    args = parse_args()
    spec = load_update_spec(args.spec)
    payload = args.payload.read_bytes()
    header = build_header(
        payload,
        spec=spec,
        app_version=args.app_version,
    )
    args.output.write_bytes(header + payload)


if __name__ == "__main__":
    main()
