# tang-primer-25k-sd-updater

Tang Primer 25K (GW5A-25A, Sipeed) + PicoRV32 向けの SD カードファームウェア更新機能。
SD カードの更新ファイルを検証して Configuration Flash の app slot を書き換え、再構成して
通常アプリへ移行するまでの一連のフローを提供します。

## 機能

- SD (FAT32) からの更新ファイル読み出しと検証 (magic / target / CRC32)
- app slot への書き込み (erase / program / readback verify)
- **書き込みスキップ**: app slot の現内容が更新ファイルと一致していれば再書き込みしない (自己修復)
- **自動移行**: SD カード無し・更新ファイル無しでもそのまま通常アプリへ移行
- 更新エラー時は updater に留まり、状態を表示

## 前提

- Tang Primer 25K (GW5A-25A)、PicoRV32 ソフトコア、Veryl、システムクロック 50 MHz
- PicoRV32 (`picorv32.v`) はリポジトリに同梱済み (ISC license)
- 詳細な前提・ボード配線は **[docs/integration-guide.md](docs/integration-guide.md)** を参照

## クイックスタート

組み込みの完全な手順 (Cargo.toml / build.rs / BoardIo 実装 / top.veryl / update_spec.toml /
ビルドフロー) はすべて **integration-guide** にあります。ここでは概要のみ示します。

### firmware (Rust)

```toml
[dependencies]
tang-primer-25k-sd-updater = { version = "0.2.0" }

[build-dependencies]
tang-primer-25k-sd-updater-build = { version = "0.2.0" }
```

`build.rs` から `update_spec.toml` の定数を生成し、`BoardIo` を実装して
`Updater::new(io, SPEC)` で起動する (完全例は integration-guide 4.1 / 5 章)。

### RTL (Veryl)

`Veryl.toml`:

```toml
[dependencies]
tang_primer_25k_sd_updater = { github = "AkiyukiOkayasu/tang-primer-25k-sd-updater", version = "0.2.0" }
```

`UpdaterCore` (PicoRV32 + TCM + レジスタを内蔵) とボード固有のピンを配線するだけ:

```veryl
inst core: tang_primer_25k_sd_updater::UpdaterCore #(
    FLASH_APP_BASE: 32'h0010_0000, // update_spec.toml の flash.app_base
    FLASH_APP_END : 32'h0020_0000, // app_base + app_size
) (
    i_clk: clk, i_rst: rst,
    o_sd_cs_n: sd_cs_n, o_sd_sclk: sd_sclk, o_sd_mosi: sd_mosi,
    i_sd_miso: sd_miso, i_sd_cd: sd_cd,
    o_flash_cs_n: flash_cs_n, o_flash_sclk: flash_sclk,
    o_flash_mosi: flash_mosi, i_flash_miso: flash_miso,
    o_reconfig_trig_n: reconfig_trig_n,
    o_state: state_enum,
);
```

完全例 (ポート宣言・enum → logic 変換・CST) は integration-guide 6 章。

### ホストツール

```sh
cargo install tang-primer-25k-sd-updater-tools
tang-primer-25k-sd-updater-tools make-update-package app.bin TANG25K.UPD --spec update_spec.toml
tang-primer-25k-sd-updater-tools make-factory-image updater.bin app.bin FACTORY.bin --spec update_spec.toml
```

## 詳細

**[docs/integration-guide.md](docs/integration-guide.md)** — BoardIo 実装例、top.veryl 完全例、
update_spec.toml、ビルドフロー、デバッグ手順。

`update_spec.toml` (更新ファイル名・target ID・Flash layout の唯一の定義) の書き方は
[update_spec.example.toml](update_spec.example.toml) とガイドの「update_spec.toml」章を参照。

## 構成

```text
crates/
├── tang-primer-25k-sd-updater/        # no_std コア (firmware に組み込む)
├── tang-primer-25k-sd-updater-build/  # build.rs 補助 (update_spec.toml → Rust 定数)
└── tang-primer-25k-sd-updater-tools/  # ホスト CLI (更新ファイル / factory image 生成)
rtl/
├── src/               # Veryl ライブラリ tang_primer_25k_sd_updater (PicoMemBus / PicoTcm /
│                      #   rst_bridge / SpiByteEngine / UpdaterRegs / UpdaterCore)
└── vendor/picorv32/   # PicoRV32 ソース (ISC license、同梱)
```

## 開発

```bash
# Rust (crates/) — fmt / lint / test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# RTL (rtl/) — Veryl の fmt / build
cd rtl
veryl fmt --check
veryl build
# RTL の Verilator 機能テストは rtl/tests/README.md を参照
```

## ライセンス

MIT OR Apache-2.0 (PicoRV32 は ISC license、`rtl/vendor/picorv32/LICENSE` 参照)
