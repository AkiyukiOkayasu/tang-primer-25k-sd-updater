#!/usr/bin/env python3
"""updater/app を固定 flash layout に配置した factory image を生成する。"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from update_spec import load_update_spec


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Create factory flash image")
    parser.add_argument("updater", type=Path, help="updater binary bitstream")
    parser.add_argument("app", type=Path, help="app binary bitstream")
    parser.add_argument("output", type=Path, help="output flash image")
    parser.add_argument("--spec", type=Path, required=True, help="update_spec.conf path")
    return parser.parse_args()


def checked_payload(path: Path, slot_name: str, slot_size: int) -> bytes:
    payload = path.read_bytes()
    if not payload:
        raise ValueError(f"{slot_name} payload is empty: {path}")
    if len(payload) > slot_size:
        raise ValueError(f"{slot_name} payload is too large: {len(payload)} > {slot_size}")
    return payload


def main() -> None:
    args = parse_args()
    spec = load_update_spec(args.spec)
    updater = checked_payload(args.updater, "updater", spec.updater_size)
    app = checked_payload(args.app, "app", spec.app_size)

    if spec.app_base < spec.updater_base + len(updater):
        raise ValueError("app image overlaps updater payload")
    if spec.app_base + len(app) > spec.flash_size_bytes:
        raise ValueError("app image exceeds flash size")

    image = bytearray([0xFF]) * spec.flash_size_bytes
    image[spec.updater_base : spec.updater_base + len(updater)] = updater
    image[spec.app_base : spec.app_base + len(app)] = app
    args.output.write_bytes(image)


if __name__ == "__main__":
    main()
