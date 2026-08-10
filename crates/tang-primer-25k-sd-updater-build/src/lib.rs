//! `update_spec.conf` からプロジェクト側の Rust 定数を生成する build 補助ライブラリ。
//!
//! プロジェクトの `build.rs` から `generate()` を呼ぶ。ホストツール
//! (`tang-primer-25k-sd-updater-tools`) は `load()` で同じ仕様を読み、package / factory image を生成する。
//! パース・検証のロジックはこの crate が唯一の実装であり、他言語実装との同期は不要。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const REQUIRED_KEYS: &[&str] = &[
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
];

/// `update_spec.conf` の内容 (ホスト側、`String` 所有)。
///
/// firmware 側の生成定数 [`tang_primer_25k_sd_updater::UpdateSpec`] とは別物。こちらは
/// `tang-primer-25k-sd-updater-tools` がファイルから直接読み込むために使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub file_name: String,
    pub magic: [u8; 8],
    pub header_size: usize,
    pub format_version: u32,
    pub target_hw_id: u32,
    pub target_fpga_id: u32,
    pub flash_layout_id: u32,
    pub flash_size_bytes: u32,
    pub updater_base: u32,
    pub updater_size: u32,
    pub app_base: u32,
    pub app_size: u32,
    pub metadata_base: u32,
    pub metadata_size: u32,
    pub golden_updater_base_candidate: u32,
    pub golden_updater_size_candidate: u32,
}

/// `update_spec.conf` の内容文字列を検証して [`Spec`] を返す。`path` はエラーメッセージ用。
pub fn parse(spec: &str, path: &str) -> Result<Spec, String> {
    let values = parse_spec(spec, path)?;
    build_spec(&values, path)
}

/// `update_spec.conf` を読み、検証して [`Spec`] を返す。
pub fn load(spec_path: &str) -> Result<Spec, String> {
    let spec = fs::read_to_string(spec_path)
        .map_err(|error| format!("{spec_path} を読み込めない: {error}"))?;
    parse(&spec, spec_path)
}

/// `update_spec.conf` を読み、`$OUT_DIR/update_spec.rs` に `SPEC` 定数を生成する。
///
/// 生成コードは `tang_primer_25k_sd_updater::UpdateSpec` を参照するため、依存 crate の名前は
/// `sd_updater` にする必要がある。`cargo:rerun-if-changed=update_spec.conf` の emit は
/// build-dependency の stdout が転送されないため、呼び出し側の build.rs で行うこと。
pub fn generate(spec_path: &str) -> Result<(), String> {
    let spec = load(spec_path)?;
    let generated = generate_rust(&spec, spec_path)?;

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("OUT_DIR が未設定")?);
    fs::write(out_dir.join("update_spec.rs"), generated)
        .map_err(|error| format!("生成した仕様を出力できない: {error}"))?;
    Ok(())
}

fn parse_spec(spec: &str, path: &str) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();

    for (line_number, line) in spec.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("{path}:{}: '=' が必要", line_number + 1))?;
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return Err(format!(
                "{path}:{}: 空の key/value は使用できない",
                line_number + 1
            ));
        }
        if values.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(format!("{path}:{}: key が重複: {key}", line_number + 1));
        }
    }

    for key in REQUIRED_KEYS {
        if !values.contains_key(*key) {
            return Err(format!("{path}: 必須 key がない: {key}"));
        }
    }
    if values.len() != REQUIRED_KEYS.len() {
        return Err(format!("{path}: 未知の key がある"));
    }
    Ok(values)
}

fn value<'a>(
    values: &'a BTreeMap<String, String>,
    key: &str,
    path: &str,
) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("{path}: 必須 key がない: {key}"))
}

fn parse_u32(values: &BTreeMap<String, String>, key: &str, path: &str) -> Result<u32, String> {
    let value = value(values, key, path)?;
    value
        .strip_prefix("0x")
        .map_or_else(|| value.parse(), |hex| u32::from_str_radix(hex, 16))
        .map_err(|_| format!("{path}: {key} は u32 ではない: {value}"))
}

fn parse_magic(values: &BTreeMap<String, String>, path: &str) -> Result<[u8; 8], String> {
    let value = value(values, "package.magic_hex", path)?;
    if value.len() != 16 {
        return Err(format!("{path}: package.magic_hex は 8 byte の 16 進数"));
    }

    let mut bytes = [0; 8];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| format!("{path}: package.magic_hex が不正"))?;
    }
    Ok(bytes)
}

fn build_spec(values: &BTreeMap<String, String>, path: &str) -> Result<Spec, String> {
    let file_name = value(values, "package.file_name", path)?;
    if !file_name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.')
    {
        return Err(format!("{path}: package.file_name が不正"));
    }

    Ok(Spec {
        file_name: file_name.to_owned(),
        magic: parse_magic(values, path)?,
        header_size: parse_u32(values, "package.header_size", path)? as usize,
        format_version: parse_u32(values, "package.format_version", path)?,
        target_hw_id: parse_u32(values, "package.target_hw_id", path)?,
        target_fpga_id: parse_u32(values, "package.target_fpga_id", path)?,
        flash_layout_id: parse_u32(values, "flash.layout_id", path)?,
        flash_size_bytes: parse_u32(values, "flash.flash_size_bytes", path)?,
        updater_base: parse_u32(values, "flash.updater_base", path)?,
        updater_size: parse_u32(values, "flash.updater_size", path)?,
        app_base: parse_u32(values, "flash.app_base", path)?,
        app_size: parse_u32(values, "flash.app_size", path)?,
        metadata_base: parse_u32(values, "flash.metadata_base", path)?,
        metadata_size: parse_u32(values, "flash.metadata_size", path)?,
        golden_updater_base_candidate: parse_u32(
            values,
            "flash.golden_updater_base_candidate",
            path,
        )?,
        golden_updater_size_candidate: parse_u32(
            values,
            "flash.golden_updater_size_candidate",
            path,
        )?,
    })
}

fn generate_rust(spec: &Spec, _path: &str) -> Result<String, String> {
    let magic = spec
        .magic
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    Ok(format!(
        "\
// このファイルは tang-primer-25k-sd-updater-build が update_spec.conf から生成する。手編集しないこと。
// 生成コードは tang_primer_25k_sd_updater::UpdateSpec を参照する。依存 crate の名前は tang_primer_25k_sd_updater にする。
pub const SPEC: tang_primer_25k_sd_updater::UpdateSpec = tang_primer_25k_sd_updater::UpdateSpec {{
    file_name: \"{file_name}\",
    magic: [{magic}],
    header_size: {header_size},
    format_version: {format_version},
    target_hw_id: {target_hw_id},
    target_fpga_id: {target_fpga_id},
    flash_layout_id: {layout_id},
    flash_size_bytes: {flash_size_bytes},
    updater_base: {updater_base},
    updater_size: {updater_size},
    app_base: {app_base},
    app_size: {app_size},
    metadata_base: {metadata_base},
    metadata_size: {metadata_size},
    golden_updater_base_candidate: {golden_updater_base_candidate},
    golden_updater_size_candidate: {golden_updater_size_candidate},
}};
",
        file_name = spec.file_name,
        header_size = spec.header_size,
        format_version = spec.format_version,
        target_hw_id = spec.target_hw_id,
        target_fpga_id = spec.target_fpga_id,
        flash_size_bytes = spec.flash_size_bytes,
        updater_base = spec.updater_base,
        updater_size = spec.updater_size,
        app_base = spec.app_base,
        app_size = spec.app_size,
        metadata_base = spec.metadata_base,
        metadata_size = spec.metadata_size,
        golden_updater_base_candidate = spec.golden_updater_base_candidate,
        golden_updater_size_candidate = spec.golden_updater_size_candidate,
        layout_id = spec.flash_layout_id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CONF: &str = "\
# sample spec
package.file_name=FPGAOSC.UPD
package.magic_hex=465047414f534300
package.header_size=0x58
package.format_version=1
package.target_hw_id=0x46504f53
package.target_fpga_id=0x47573525
flash.flash_size_bytes=0x800000
flash.updater_base=0x000000
flash.updater_size=0x100000
flash.app_base=0x100000
flash.app_size=0x100000
flash.metadata_base=0x200000
flash.metadata_size=0x010000
flash.golden_updater_base_candidate=0x700000
flash.golden_updater_size_candidate=0x100000
flash.layout_id=0x46504f31
";

    #[test]
    fn parses_valid_spec() {
        let spec = build_spec(
            &parse_spec(SAMPLE_CONF, "update_spec.conf").unwrap(),
            "update_spec.conf",
        )
        .unwrap();
        assert_eq!(spec.file_name, "FPGAOSC.UPD");
        assert_eq!(spec.app_base, 0x100000);
        assert_eq!(spec.header_size, 0x58);
        assert_eq!(spec.magic, [0x46, 0x50, 0x47, 0x41, 0x4F, 0x53, 0x43, 0x00]);
    }

    #[test]
    fn rejects_missing_key() {
        let spec = SAMPLE_CONF.replace("flash.layout_id=0x46504f31\n", "");
        assert!(parse_spec(&spec, "update_spec.conf").is_err());
    }

    #[test]
    fn rejects_unknown_key() {
        let spec = format!("{SAMPLE_CONF}extra.key=1\n");
        assert!(parse_spec(&spec, "update_spec.conf").is_err());
    }

    #[test]
    fn rejects_invalid_file_name() {
        let spec = SAMPLE_CONF.replace("package.file_name=FPGAOSC.UPD", "package.file_name=../x");
        let values = parse_spec(&spec, "update_spec.conf").unwrap();
        assert!(build_spec(&values, "update_spec.conf").is_err());
    }

    #[test]
    fn generates_const_with_crate_reference() {
        let spec = build_spec(
            &parse_spec(SAMPLE_CONF, "update_spec.conf").unwrap(),
            "update_spec.conf",
        )
        .unwrap();
        let generated = generate_rust(&spec, "update_spec.conf").unwrap();
        assert!(generated.contains("pub const SPEC: tang_primer_25k_sd_updater::UpdateSpec"));
        assert!(generated.contains("file_name: \"FPGAOSC.UPD\""));
    }
}
