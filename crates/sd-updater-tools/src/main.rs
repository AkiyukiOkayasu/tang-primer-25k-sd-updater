//! SD updater のホストツール。
//!
//! `update_spec.conf` から package (FPGAOSC.UPD) と factory flash image を生成する。
//! CRC32 / SHA256 は共有 crate `sd-updater` を再利用し、spec のパース・検証は
//! `sd-updater-build` に集約されている。

use std::env;
use std::fs;
use std::path::PathBuf;

use sd_updater::crc32;
use sd_updater_build::Spec;

fn main() {
    let args: Vec<String> = env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        None | Some("help" | "--help" | "-h") => {
            print_usage();
            Ok(())
        }
        Some("make-update-package") => run_make_update_package(&args[2..]),
        Some("make-factory-image") => run_make_factory_image(&args[2..]),
        Some(command) => {
            eprintln!("error: 未知のコマンド: {command}");
            print_usage();
            Err(String::new())
        }
    };
    if let Err(message) = result {
        if !message.is_empty() {
            eprintln!("error: {message}");
        }
        std::process::exit(1);
    }
}

fn print_usage() {
    println!(
        "SD updater ホストツール\n\
         \n\
         Usage:\n\
           sd-updater-tools make-update-package <payload> <output> --spec <update_spec.conf> [--app-version N]\n\
           sd-updater-tools make-factory-image <updater> <app> <output> --spec <update_spec.conf>\n\
         \n\
         --app-version は 10 進数または 0x 接頭辞付き 16 進数 (既定 0)"
    );
}

struct CommonArgs {
    positionals: Vec<PathBuf>,
    spec: PathBuf,
    app_version: Option<String>,
}

fn parse_common_args(args: &[String]) -> Result<CommonArgs, String> {
    let mut positionals = Vec::new();
    let mut spec: Option<PathBuf> = None;
    let mut app_version: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--spec" => {
                index += 1;
                let value = args.get(index).ok_or("--spec の値がありません")?;
                spec = Some(PathBuf::from(value));
            }
            "--app-version" => {
                index += 1;
                let value = args.get(index).ok_or("--app-version の値がありません")?;
                app_version = Some(value.clone());
            }
            argument if argument.starts_with("--") => {
                return Err(format!("未知のオプション: {argument}"));
            }
            positional => positionals.push(PathBuf::from(positional)),
        }
        index += 1;
    }

    Ok(CommonArgs {
        positionals,
        spec: spec.ok_or("--spec <update_spec.conf> が必須です")?,
        app_version,
    })
}

/// Python の `int(value, 0)` と同等の base-0 解釈 (0x 接頭辞 → 16 進、それ以外は 10 進)。
fn parse_app_version(value: &str) -> Result<u32, String> {
    value
        .strip_prefix("0x")
        .map_or_else(|| value.parse(), |hex| u32::from_str_radix(hex, 16))
        .map_err(|_| format!("--app-version が u32 ではありません: {value}"))
}

fn run_make_update_package(args: &[String]) -> Result<(), String> {
    let common = parse_common_args(args)?;
    if common.positionals.len() != 2 {
        return Err("make-update-package には <payload> <output> の 2 引数が必要です".to_owned());
    }
    let payload = &common.positionals[0];
    let output = &common.positionals[1];
    let app_version = common
        .app_version
        .as_deref()
        .map(parse_app_version)
        .transpose()?
        .unwrap_or(0);

    let spec = sd_updater_build::load(common.spec.to_str().ok_or("spec パスが不正")?)?;
    let bytes = build_update_package(payload, spec, app_version)?;
    fs::write(output, bytes)
        .map_err(|error| format!("{} を書き出せない: {error}", output.display()))?;
    Ok(())
}

fn build_update_package(
    payload_path: &PathBuf,
    spec: Spec,
    app_version: u32,
) -> Result<Vec<u8>, String> {
    let payload = fs::read(payload_path)
        .map_err(|error| format!("{} を読み込めない: {error}", payload_path.display()))?;
    if payload.is_empty() {
        return Err("payload is empty".to_owned());
    }
    if payload.len() > spec.app_size as usize {
        return Err(format!(
            "payload is too large for app slot: {} > {}",
            payload.len(),
            spec.app_size
        ));
    }
    if spec.header_size < 0x48 {
        return Err(format!("header_size が小さすぎます: {}", spec.header_size));
    }

    let payload_crc32 = crc32::checksum(&payload);

    let mut header = vec![0u8; spec.header_size];
    header[0..8].copy_from_slice(&spec.magic);
    write_u32_le(&mut header, 0x08, spec.format_version);
    write_u32_le(&mut header, 0x0C, spec.target_hw_id);
    write_u32_le(&mut header, 0x10, spec.target_fpga_id);
    write_u32_le(&mut header, 0x14, spec.flash_layout_id);
    write_u32_le(&mut header, 0x18, app_version);
    write_u32_le(&mut header, 0x1C, spec.header_size as u32);
    write_u32_le(&mut header, 0x20, payload.len() as u32);
    write_u32_le(&mut header, 0x24, payload_crc32);
    // 0x28..0x48 は旧 SHA256 フィールドの予約領域 (ゼロ埋め)

    let mut out = header;
    out.extend_from_slice(&payload);
    Ok(out)
}

fn run_make_factory_image(args: &[String]) -> Result<(), String> {
    let common = parse_common_args(args)?;
    if common.positionals.len() != 3 {
        return Err(
            "make-factory-image には <updater> <app> <output> の 3 引数が必要です".to_owned(),
        );
    }
    let updater = &common.positionals[0];
    let app = &common.positionals[1];
    let output = &common.positionals[2];

    let spec = sd_updater_build::load(common.spec.to_str().ok_or("spec パスが不正")?)?;
    let bytes = build_factory_image(updater, app, &spec)?;
    fs::write(output, bytes)
        .map_err(|error| format!("{} を書き出せない: {error}", output.display()))?;
    Ok(())
}

fn build_factory_image(
    updater_path: &PathBuf,
    app_path: &PathBuf,
    spec: &Spec,
) -> Result<Vec<u8>, String> {
    let updater = fs::read(updater_path)
        .map_err(|error| format!("{} を読み込めない: {error}", updater_path.display()))?;
    let app = fs::read(app_path)
        .map_err(|error| format!("{} を読み込めない: {error}", app_path.display()))?;

    if updater.is_empty() {
        return Err(format!(
            "updater payload is empty: {}",
            updater_path.display()
        ));
    }
    if app.is_empty() {
        return Err(format!("app payload is empty: {}", app_path.display()));
    }
    if updater.len() > spec.updater_size as usize {
        return Err(format!(
            "updater payload is too large: {} > {}",
            updater.len(),
            spec.updater_size
        ));
    }
    if app.len() > spec.app_size as usize {
        return Err(format!(
            "app payload is too large: {} > {}",
            app.len(),
            spec.app_size
        ));
    }

    let updater_end = u64::from(spec.updater_base) + updater.len() as u64;
    if u64::from(spec.app_base) < updater_end {
        return Err("app image overlaps updater payload".to_owned());
    }
    if u64::from(spec.app_base) + app.len() as u64 > u64::from(spec.flash_size_bytes) {
        return Err("app image exceeds flash size".to_owned());
    }

    let mut image = vec![0xFFu8; spec.flash_size_bytes as usize];
    let updater_start = spec.updater_base as usize;
    image[updater_start..updater_start + updater.len()].copy_from_slice(&updater);
    let app_start = spec.app_base as usize;
    image[app_start..app_start + app.len()].copy_from_slice(&app);
    Ok(image)
}

fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset] = value as u8;
    bytes[offset + 1] = (value >> 8) as u8;
    bytes[offset + 2] = (value >> 16) as u8;
    bytes[offset + 3] = (value >> 24) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

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

    fn test_spec() -> Spec {
        sd_updater_build::parse(SAMPLE_CONF, "update_spec.conf").unwrap()
    }

    fn temp_dir() -> PathBuf {
        let mut dir = env::temp_dir();
        dir.push(format!("sd_updater_tools_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
        let path = temp_dir().join(name);
        File::create(&path).unwrap().write_all(bytes).unwrap();
        path
    }

    fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn update_package_header_layout_matches_contract() {
        let payload = write_temp("payload.bin", &[0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03]);
        let out = build_update_package(&payload, test_spec(), 0x123).unwrap();

        assert_eq!(out.len(), 0x58 + 7);
        assert_eq!(&out[0..8], &test_spec().magic);
        assert_eq!(read_u32_le(&out, 0x08), 1);
        assert_eq!(read_u32_le(&out, 0x0C), 0x4650_4F53);
        assert_eq!(read_u32_le(&out, 0x10), 0x4757_3525);
        assert_eq!(read_u32_le(&out, 0x14), 0x4650_4F31);
        assert_eq!(read_u32_le(&out, 0x18), 0x123);
        assert_eq!(read_u32_le(&out, 0x1C), 0x58);
        assert_eq!(read_u32_le(&out, 0x20), 7);
        assert_eq!(
            read_u32_le(&out, 0x24),
            crc32::checksum(&[0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03])
        );
        assert_eq!(&out[0x28..0x48], &[0u8; 32]); // 旧 SHA256 フィールドは予約 (ゼロ埋め)
    }

    #[test]
    fn update_package_rejects_empty_and_oversized_payload() {
        let empty = write_temp("empty.bin", &[]);
        assert!(build_update_package(&empty, test_spec(), 0).is_err());

        let mut spec = test_spec();
        spec.app_size = 4;
        let big = write_temp("big.bin", &[0; 8]);
        assert!(build_update_package(&big, spec, 0).is_err());
    }

    #[test]
    fn factory_image_places_slots_on_ff_background() {
        let updater = write_temp("updater.bin", &[0xAA; 0x10]);
        let app = write_temp("app.bin", &[0x55; 0x20]);
        let image = build_factory_image(&updater, &app, &test_spec()).unwrap();

        assert_eq!(image.len(), 0x800_000);
        assert_eq!(&image[0x000000..0x000010], &[0xAA; 0x10]);
        assert_eq!(&image[0x100000..0x100020], &[0x55; 0x20]);
        assert_eq!(image[0x000010], 0xFF);
        assert_eq!(image[0x0F_FFFF], 0xFF);
        assert_eq!(image[0x100020], 0xFF);
        assert_eq!(image[0x7F_FFFF], 0xFF);
    }

    #[test]
    fn factory_image_rejects_overlap_and_overflow() {
        let updater = write_temp("updater.bin", &[0xAA; 0x10]);
        let app = write_temp("app.bin", &[0x55; 0x20]);

        let mut overlap = test_spec();
        overlap.app_base = 0x000008;
        assert!(build_factory_image(&updater, &app, &overlap).is_err());

        let mut overflow = test_spec();
        overflow.app_base = 0x7F_FFF0;
        assert!(build_factory_image(&updater, &app, &overflow).is_err());

        let empty = write_temp("empty.bin", &[]);
        assert!(build_factory_image(&empty, &app, &test_spec()).is_err());
    }

    #[test]
    fn app_version_parses_base0_like_python() {
        assert_eq!(parse_app_version("0").unwrap(), 0);
        assert_eq!(parse_app_version("123").unwrap(), 123);
        assert_eq!(parse_app_version("0x123").unwrap(), 0x123);
        assert!(parse_app_version("xyz").is_err());
        assert!(parse_app_version("0x1_0000_0000").is_err());
    }
}
