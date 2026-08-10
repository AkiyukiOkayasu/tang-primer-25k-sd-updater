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

`build.rs` (プロジェクト固有の `update_spec.conf` から定数を生成):

```rust
fn main() {
    println!("cargo:rerun-if-changed=update_spec.conf");
    tang_primer_25k_sd_updater_build::generate("update_spec.conf").expect("update_spec.conf を読み込めない");
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

`top.veryl` でモジュールを配線し、パラメータを渡す (数値は FPGA_Oscillator での実例):

```veryl
inst regs: tang_primer_25k_sd_updater_UpdaterRegs #(
    BASE          : 32'h03_0000,
    FLASH_APP_BASE: 32'h0010_0000,
    FLASH_APP_END : 32'h0020_0000,
) ( ... );

inst tcm: tang_primer_25k_sd_updater::PicoTcm #(
    ADDR_WIDTH: 15,
    HEX_FILE  : "updater.hex",
) ( ... );
```

## 動作確認

- [FPGA_Oscillator](https://github.com/AkiyukiOkayasu/FPGA_Oscillator) (Eurorack oscillator) で実機検証済み:
  updater を 0x000000 / app を 0x100000 に配置した MultiBoot 構成、システムクロック 50 MHz

### ホストツール

```sh
cargo install tang-primer-25k-sd-updater-tools
tang-primer-25k-sd-updater-tools make-update-package app.bin FPGAOSC.UPD --spec update_spec.conf
tang-primer-25k-sd-updater-tools make-factory-image updater.bin app.bin FACTORY.bin --spec update_spec.conf
```

## 詳細

実装・ブリングアップの詳細は **[docs/integration-guide.md](docs/integration-guide.md)**
(BoardIo 実装例、top.veryl 完全例、ビルドフロー、デバッグ手順) を参照してください。

`update_spec.conf` (更新ファイル名・target ID・Flash layout の唯一の定義) の書き方は
ガイドの「update_spec.conf」章を参照してください。

## 構成

```text
crates/
├── tang-primer-25k-sd-updater/        # no_std コア (firmware に組み込む)
├── tang-primer-25k-sd-updater-build/  # build.rs 補助 (update_spec.conf → Rust 定数)
└── tang-primer-25k-sd-updater-tools/  # ホスト CLI (更新ファイル / factory image 生成)
rtl/
└── src/               # Veryl ライブラリ tang_primer_25k_sd_updater (RTL モジュール群)
```

## 開発

```bash
just check    # fmt / clippy / test / RTL ビルド・検証
just rtl-check # rtl/ のみの検証
```

## ライセンス

MIT OR Apache-2.0
