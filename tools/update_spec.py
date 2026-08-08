"""SD updater と host tool で共有する update format / flash layout の仕様を読み込む。

仕様ファイルは各プロジェクトが保持する `update_spec.conf` を `--spec` で渡す。
REQUIRED_KEYS は `crates/sd-updater-build/src/lib.rs` と同期すること。
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

REQUIRED_KEYS = frozenset(
    {
        "package.file_name",
        "package.magic_hex",
        "package.header_size",
        "package.format_version",
        "package.target_hw_id",
        "package.target_fpga_id",
        "flash.flash_size_bytes",
        "flash.updater_base",
        "flash.updater_size",
        "flash.app_base",
        "flash.app_size",
        "flash.metadata_base",
        "flash.metadata_size",
        "flash.golden_updater_base_candidate",
        "flash.golden_updater_size_candidate",
        "flash.layout_id",
    }
)


@dataclass(frozen=True)
class UpdateSpec:
    file_name: str
    magic: bytes
    header_size: int
    format_version: int
    target_hw_id: int
    target_fpga_id: int
    flash_size_bytes: int
    updater_base: int
    updater_size: int
    app_base: int
    app_size: int
    metadata_base: int
    metadata_size: int
    golden_updater_base_candidate: int
    golden_updater_size_candidate: int
    flash_layout_id: int


def _parse_values(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line_number, line in enumerate(path.read_text().splitlines(), start=1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            raise ValueError(f"{path}:{line_number}: '=' が必要です")
        key, value = (part.strip() for part in line.split("=", maxsplit=1))
        if not key or not value:
            raise ValueError(f"{path}:{line_number}: 空の key/value は使用できません")
        if key in values:
            raise ValueError(f"{path}:{line_number}: key が重複しています: {key}")
        values[key] = value

    if set(values) != REQUIRED_KEYS:
        missing = REQUIRED_KEYS - set(values)
        unknown = set(values) - REQUIRED_KEYS
        raise ValueError(f"{path}: key が不正です: missing={missing}, unknown={unknown}")
    return values


def _u32(values: dict[str, str], key: str, path: Path) -> int:
    value = int(values[key], 0)
    if not 0 <= value <= 0xFFFF_FFFF:
        raise ValueError(f"{path}: {key} は u32 の範囲外です")
    return value


def load_update_spec(spec_path: Path) -> UpdateSpec:
    """プロジェクトの update_spec.conf を検証し、host tool 用の値として返す。"""
    values = _parse_values(spec_path)
    file_name = values["package.file_name"]
    if not file_name.replace(".", "").isalnum():
        raise ValueError(f"{spec_path}: package.file_name が不正です")

    try:
        magic = bytes.fromhex(values["package.magic_hex"])
    except ValueError as error:
        raise ValueError(f"{spec_path}: package.magic_hex が不正です") from error
    if len(magic) != 8:
        raise ValueError(f"{spec_path}: package.magic_hex は 8 byte 必須です")

    return UpdateSpec(
        file_name=file_name,
        magic=magic,
        header_size=_u32(values, "package.header_size", spec_path),
        format_version=_u32(values, "package.format_version", spec_path),
        target_hw_id=_u32(values, "package.target_hw_id", spec_path),
        target_fpga_id=_u32(values, "package.target_fpga_id", spec_path),
        flash_size_bytes=_u32(values, "flash.flash_size_bytes", spec_path),
        updater_base=_u32(values, "flash.updater_base", spec_path),
        updater_size=_u32(values, "flash.updater_size", spec_path),
        app_base=_u32(values, "flash.app_base", spec_path),
        app_size=_u32(values, "flash.app_size", spec_path),
        metadata_base=_u32(values, "flash.metadata_base", spec_path),
        metadata_size=_u32(values, "flash.metadata_size", spec_path),
        golden_updater_base_candidate=_u32(values, "flash.golden_updater_base_candidate", spec_path),
        golden_updater_size_candidate=_u32(values, "flash.golden_updater_size_candidate", spec_path),
        flash_layout_id=_u32(values, "flash.layout_id", spec_path),
    )
