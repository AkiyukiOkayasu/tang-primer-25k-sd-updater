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

- ボード: Tang Primer 25K (GW5A-25A MBGA121N)。オンボード 8MB Configuration Flash (W25Q64JV 系) を
  CFG/MSPI ピン (E6=MCS_N, E7=CCLK, D6=MOSI, E5=MISO) から user logic で直接操作する
- SD カード: SPI モード接続 (Pmod MicroSD)
- reconfig: MultiBoot + `RECONFIG_N` への Low pulse。`RECONFIG_N` はボードで外部プルアップ済みのため、
  トリガーはオープンドレインでショートする
- PicoRV32 ソフトコア + Veryl。firmware は `PicoTcm.ADDR_WIDTH` の TCM (32KB = 15bit) に収める。
  動作検証は 50 MHz で実施

## クイックスタート

### Rust (firmware)

`Cargo.toml`:

```toml
[dependencies]
tang_primer_25k_sd_updater = { version = "0.1.0" }

[build-dependencies]
tang-primer-25k-sd-updater-build = { version = "0.1.0" }
```

`build.rs` (プロジェクト固有の `update_spec.toml` から定数を生成):

```rust
fn main() {
    println!("cargo:rerun-if-changed=update_spec.toml");
    tang_primer_25k_sd_updater_build::generate("update_spec.toml").expect("update_spec.toml を読み込めない");
}
```

`BoardIo` を実装して起動:

```rust
include!(concat!(env!("OUT_DIR"), "/update_spec.rs"));

let mut updater = tang_primer_25k_sd_updater::Updater::new(MyBoardIo::new(), SPEC);
loop {
    let status = updater.poll_once();
    updater.report_status(status);
}
```

### RTL (Veryl)

`Veryl.toml`:

```toml
[dependencies]
tang_primer_25k_sd_updater = { version = "0.1.0" }
```

`top.veryl` は `UpdaterCore` (PicoRV32 + TCM + レジスタを内蔵) とボード固有の
SD / Flash / reconfig ピンを配線するだけ:

```veryl
var state_enum: tang_primer_25k_sd_updater::updater_pkg::UpdaterState;
inst core: tang_primer_25k_sd_updater::UpdaterCore #(
    TCM_ADDR_WIDTH: 15,
    HEX_FILE      : "updater.hex",
    BASE          : 32'h03_0000,
    FLASH_APP_BASE: 32'h0010_0000,
    FLASH_APP_END : 32'h0020_0000,
) (
    i_clk: clk, i_rst: rst,
    o_sd_cs_n: sd_cs_n, o_sd_sclk: sd_sclk, o_sd_mosi: sd_mosi,
    i_sd_miso: sd_miso, i_sd_cd: sd_cd,
    o_flash_cs_n: flash_cs_n, o_flash_sclk: flash_sclk,
    o_flash_mosi: flash_mosi, i_flash_miso: flash_miso,
    o_reconfig_trig_n: reconfig_trig_n,
    o_state: state_enum,
);

// enum → logic は assign で暗黙変換 (ピンへ出すときに変換する)
assign state = state_enum;
```

PicoRV32 (`rtl/vendor/picorv32/picorv32.v`, ISC license) はリポジトリに同梱済み。
Gowin プロジェクトのファイルリストに追加すること (veryl build の生成物には含まれない)。

### ホストツール

```sh
cargo install tang-primer-25k-sd-updater-tools
tang-primer-25k-sd-updater-tools make-update-package app.bin TANG25K.UPD --spec update_spec.toml
tang-primer-25k-sd-updater-tools make-factory-image updater.bin app.bin FACTORY.bin --spec update_spec.toml
```

## 詳細

実装・ブリングアップの詳細は **[docs/integration-guide.md](docs/integration-guide.md)**
(BoardIo 実装例、top.veryl 完全例、ビルドフロー、デバッグ手順) を参照してください。

`update_spec.toml` (更新ファイル名・target ID・Flash layout の唯一の定義) の書き方は
[update_spec.example.toml](update_spec.example.toml) とガイドの「update_spec.toml」章を参照してください。

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

MIT OR Apache-2.0
