# 統合ガイド (別プロジェクト向け)

このガイドは、このリポジトリ (tang-primer-25k-sd-updater) を**Tang Primer 25K を使った別の FPGA プロジェクトに組み込む**
ための手順です。このリポジトリだけを見て、firmware + RTL + ビルドフローまで実装できることを
目的とします。

前提: **Tang Primer 25K (GW5A-25A MBGA121N, Arora V)** + **PicoRV32 ソフトコア** + **Veryl**。
他の GW5A 系ボードへ移植する場合は、Configuration Flash のピンアサインと MultiBoot 設定を確認する。
再構成は MultiBoot + `RECONFIG_N` Low pulse を使用します (配線は 9.2)。
システムクロック 50 MHz 前提 (firmware の delay は 50 MHz 換算)。
updater firmware は TCM 32KB 以内、SD カードは FAT32。

---

## 1. アーキテクチャ概要

```text
                    ┌─────────────────────────────────────────────┐
                    │ FPGA (Tang Primer 25K)                       │
  SD カード         │  ┌──────────────┐    ┌──────────────────┐    │
  (FAT32) ──SPI──▶  │  │ updater RTL  │    │ 通常 app RTL     │    │
                    │  │  (top.veryl) │    │  (別プロジェクト) │    │
                    │  │              │    │                  │    │
                    │  │ PicoRV32     │◀──▶│                  │    │
                    │  │ + firmware   │    │                  │    │
                    │  │ (TCM に常駐) │    │                  │    │
                    │  └──────┬───────┘    └────────┬─────────┘    │
                    │         │ SPI (MSPI)          │              │
                    └─────────┼─────────────────────┼──────────────┘
                              ▼                     ▼
                    ┌─────────────────────────────────────┐
                    │ オンボード 8MB SPI Flash (W25Q64JV)   │
                    │ 0x000000 updater bitstream           │
                    │ 0x100000 app bitstream (更新対象)     │
                    └─────────────────────────────────────┘
```

- 電源投入時は必ず **0x000000 の updater** が起動する
- updater が SD カードの更新ファイルを検証し、必要なら **app slot を書き換える**
- 完了後 (または更新不要時) は `reconfig_trig_n` の Low pulse で再構成し、
  MultiBoot 設定に従って **0x100000 の app** を起動する

### 構成要素

| 要素 | このリポジトリの場所 | プロジェクト側で作るもの |
| --- | --- | --- |
| firmware コア | `crates/tang-primer-25k-sd-updater` (no_std) | `BoardIo` (firmware と MMIO を繋ぐトレイト、実装例は 5 章) + `main.rs` + `update_spec.toml` |
| ビルド補助 | `crates/tang-primer-25k-sd-updater-build` | `build.rs` から呼ぶ |
| ホストツール | `crates/tang-primer-25k-sd-updater-tools` | なし (CLI として使用) |
| RTL ライブラリ | `rtl/` (Veryl `tang_primer_25k_sd_updater`) | `top.veryl` (配線のみ) |
| Verilator テスト | `rtl/tests/` | なし |

---

## 2. 更新フロー (firmware の動作)

`Updater::poll_once()` が 1 回呼ばれると、以下のフローが最後まで進む:

1. `open_sd_card()` — SD カードを初期化
   - `sd_card_detect()` で未挿入を即判定 (未挿入なら即エラー → app へ移行)
   - embedded-sdmmc で初期化し、FAT ボリュームをマウント
2. `read_header()` — 固定ファイル名 (`UpdateSpec.file_name`) の header を読み、
   `update_spec.toml` 由来の生成定数 (SPEC) と一致するか検証
   (magic / format_version / target_hw_id / target_fpga_id / flash_layout_id / payload_size。詳細は 3 章)
3. **skip 判定**: app slot の現内容の **CRC32** を header の `payload_crc32` と比較
   - 一致 → 書き込みをスキップして reconfig (SD payload は読まない)
   - 不一致 → 次へ
4. `validate_payload_digest()` — SD payload を読み、CRC32 を検証
5. `program_app_slot()` — erase (64KB ブロック) → program (256B ページ) → readback verify
6. `trigger_app_reconfig()` — `reconfig_trig_n` を 1ms Low pulse → MultiBoot で app 起動

- **検証・書き込みエラー** (header / target / payload / flash) 時は updater に留まり、
  `UpdateStatus` (state 表示) で状態を報告する
- **SD 無し / ファイル無し / カード読めず** はエラー扱いにせず、そのまま reconfig して
  app へ移行する (skip と同じ経路)

---

## 3. パッケージ形式 (TANG25K.UPD)

ホストツール `tang-primer-25k-sd-updater-tools make-update-package` が生成する形式。
header (0x58 = 88 bytes) + payload (app bitstream) の連結。

```sh
tang-primer-25k-sd-updater-tools make-update-package app.bin TANG25K.UPD --spec update_spec.toml
```

`app.bin` は app のビットストリーム (Gowin 合成の出力 .bin/.fs をそのまま使う)。
`updater.bin` / `FACTORY.bin` は 8.1 と 8.3 で扱う。

| offset | サイズ | フィールド | 説明 |
| --- | --- | --- | --- |
| 0x00 | 8 | magic | ファイル形式識別子 (プロジェクト固有) |
| 0x08 | 4 | format_version | パッケージ形式バージョン (現行 1) |
| 0x0C | 4 | target_hw_id | 製品識別子 (プロジェクト固有) |
| 0x10 | 4 | target_fpga_id | 対象 FPGA 識別子 |
| 0x14 | 4 | flash_layout_id | フラッシュ配置識別子 (プロジェクト固有) |
| 0x18 | 4 | app_version | アプリバージョン (`make-update-package` の `--app-version` で指定。情報のみ) |
| 0x1C | 4 | payload_offset | payload のオフセット (= header_size) |
| 0x20 | 4 | payload_size | payload のサイズ |
| 0x24 | 4 | payload_crc32 | payload の CRC32 |
| 0x28 | 32 | reserved | 予約領域 (ゼロ埋め) |

値の基準は常にプロジェクトの `update_spec.toml`。

- `header_size` は **0x48 以上**であれば任意 (0x28 以降はゼロ埋めの予約領域。
  サンプルでは 0x58 を 4 バイト境界アライメントで用いる)

---

## 4. プロジェクトへの追加手順

crates は crates.io、RTL ライブラリは Veryl registry から取得する (ローカルに
共有 repo を clone する必要はない)。

### 4.1 Cargo (firmware)

`Cargo.toml`:

```toml
[dependencies]
tang_primer_25k_sd_updater = { version = "0.1" }

[build-dependencies]
tang-primer-25k-sd-updater-build = { version = "0.1" }

[target.'cfg(target_arch = "riscv32")'.dependencies]
panic-halt = "1.0.0"
riscv-rt = { version = "0.17.1", features = ["memory", "single-hart", "no-mhartid", "no-xie-xip", "no-xtvec"] }
```

- `update_spec.toml` は 4.3 で定義する (build.rs がビルド時に参照)

`.cargo/config.toml` (riscv ターゲット固定):

```toml
[build]
target = "riscv32imc-unknown-none-elf"

[target.riscv32imc-unknown-none-elf]
rustflags = ["-C", "link-arg=-Tlink.x"]
# RISC-V binary cannot run on host.
runner = "false"
```

`memory.x` (riscv-rt の link.x が参照する。TCM 32KB = RAM 28K + STACK 4K):

```text
MEMORY
{
    RAM   : ORIGIN = 0x00000000, LENGTH = 28K
    STACK : ORIGIN = 0x00007000, LENGTH = 4K
}

REGION_ALIAS("REGION_TEXT", RAM);
REGION_ALIAS("REGION_RODATA", RAM);
REGION_ALIAS("REGION_DATA", RAM);
REGION_ALIAS("REGION_BSS", RAM);
REGION_ALIAS("REGION_HEAP", RAM);
REGION_ALIAS("REGION_STACK", STACK);
```

- RAM+STACK の合計は TCM の固定サイズ 32KB (15bit) と一致させる
  (TCM サイズの固定化の理由は 6 章参照)
- memory.x を OUT_DIR へコピーし、`cargo:rustc-link-search` を出す (riscv-rt の link.x が参照する)

`build.rs`:

```rust
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rerun-if-changed=update_spec.toml");
    tang_primer_25k_sd_updater_build::generate("update_spec.toml")
        .expect("update_spec.toml を読み込めない");
}
```

`main.rs` (entry。`mmio` モジュールは 5 章のテンプレートで実装する):

```rust
#![no_std]
#![no_main]

mod mmio;

use panic_halt as _; // panic 時のハンドラ (リンクに必要)

use mmio::UpdaterMmio;
use tang_primer_25k_sd_updater::Updater;

include!(concat!(env!("OUT_DIR"), "/update_spec.rs"));

#[unsafe(export_name = "_setup_interrupts")]
fn setup_interrupts() {}

#[riscv_rt::entry]
fn main() -> ! {
    let mmio = unsafe { UpdaterMmio::new() };
    let mut updater = Updater::new(mmio, SPEC);
    loop {
        let status = updater.poll_once();
        updater.report_status(status);
    }
}
```

- `include!` で生成された `SPEC` を使う (生成コードは `tang_primer_25k_sd_updater::UpdateSpec` を参照するため、
  依存 crate の名前は `tang_primer_25k_sd_updater` で固定)
- PicoRV32 は CSR 命令を使わない前提 (riscv-rt の割り込み初期化を空実装にする)

### 4.2 Veryl (RTL)

`Veryl.toml`:

```toml
[project]
name = "fpga_updater"
version = "0.1.0"
description = "SD card updater"
authors = ["あなた"]

[build]
sources = ["src"]
clock_type = "posedge"
reset_type = "sync_high"
target = { type = "directory", path = "target/" }

[dependencies]
tang_primer_25k_sd_updater = { version = "0.1.0" }
```

- `reset_type = "sync_high"` は生成物の既定。top の `rst` ポートは
  `reset_async_high` で宣言し、`UpdaterCore` 内部の rst_bridge が
  非同期アサート → 同期デアサートへ変換する (6 章の例参照)

### 4.3 update_spec.toml

プロジェクト固有の値 (hw_id / flash layout) を定義する唯一のファイル。
下記を雛形として、プロジェクトの値に書き換える (リポジトリの `update_spec.example.toml` に同内容のサンプルがある)。
キー集合と構文は tang-primer-25k-sd-updater-build のドキュメントを参照。

```toml
file_name = "TANG25K.UPD"
magic_hex = "54414e4732354b00"
header_size = 0x58
format_version = 1
target_hw_id = 0x5432354b
target_fpga_id = 0x47573541

[flash]
flash_size_bytes = 0x800000
updater_base = 0x000000
updater_size = 0x100000
app_base = 0x100000
app_size = 0x100000
metadata_base = 0x200000
metadata_size = 0x010000
golden_updater_base_candidate = 0x700000
golden_updater_size_candidate = 0x100000
layout_id = 0x4c415931
```

各キーの役割:

| キー | 役割 |
| --- | --- |
| `file_name` | SD から読む固定ファイル名 (FAT32 8.3: 8 文字 + 拡張子 3 文字) |
| `magic_hex` | header 先頭 8 byte の 16 進数 (16 文字) |
| `header_size` / `format_version` | header サイズ / パッケージ形式バージョン (3 章) |
| `target_hw_id` / `target_fpga_id` | 製品 / FPGA 識別子 (3 章) |
| `flash.flash_size_bytes` | Configuration Flash の容量 |
| `flash.updater_base/size` | updater bitstream を置く領域 (通常 0x000000 から) |
| `flash.app_base/size` | 更新対象の app slot。firmware の書き込み先と `UpdaterCore.FLASH_APP_BASE/END` に一致させる |
| `flash.metadata_*` | 将来の適用済み管理用に予約 (現行フローでは未使用) |
| `flash.golden_updater_*` | 将来の Golden fallback 用に予約 (現行フローでは未使用) |
| `flash.layout_id` | レイアウト識別子 (2 章参照) |

---

## 5. BoardIo 実装 (テンプレート)

`BoardIo` は SD byte SPI / Flash 操作 / reconfig トリガを MMIO レジスタに接続する。
以下のテンプレートがレジスタ配置の契約であり、RTL 側 (`UpdaterRegs`) と一致する。
レジスタ配置を変える場合は firmware と RTL の両方を同期する。

```rust
use core::ptr::{read_volatile, write_volatile};
use tang_primer_25k_sd_updater::{BoardIo, IoError};

// PicoMemBus の peripheral 窓 (bit22 = 0x0040_0000) + UpdaterRegs の BASE (0x0003_0000)
pub const UPDATER_PERIPH_BASE: usize = 0x0043_0000;

// updaterRegs.veryl の ADDR_* - BASE と一致させる
const REG_STATUS: usize = 0x00;   // bit0 = SD card detect (生レベル)
const REG_STATE: usize = 0x0C;
const REG_SD_CONTROL: usize = 0x10;  // bit0 START / bit1 CS_ASSERT
const REG_SD_STATUS: usize = 0x14;   // bit0 BUSY / bit1 ERROR
const REG_SD_CLK_DIV: usize = 0x18;
const REG_SD_TX: usize = 0x1C;
const REG_SD_RX: usize = 0x20;
const REG_FLASH_ADDRESS: usize = 0x30;
const REG_FLASH_LENGTH: usize = 0x34;
const REG_FLASH_COMMAND: usize = 0x38;
const REG_FLASH_STATUS: usize = 0x3C;
const REG_FLASH_JEDEC_ID: usize = 0x40;
const REG_RECONFIG_CONTROL: usize = 0x44; // bit0 ASSERT_LOW
const REG_FLASH_BUFFER: usize = 0x300;    // 256 bytes (64 words)

const SD_CONTROL_START: u32 = 1 << 0;
const SD_CONTROL_CS_ASSERT: u32 = 1 << 1;
const STATUS_SD_CARD_DETECT: u32 = 1 << 0;
const FLASH_COMMAND_ERASE_64K: u32 = 1;
const FLASH_COMMAND_PROGRAM_PAGE: u32 = 2;
const FLASH_COMMAND_READ: u32 = 3;
const FLASH_COMMAND_READ_JEDEC_ID: u32 = 4;
const RECONFIG_CONTROL_ASSERT_LOW: u32 = 1 << 0;
const BUSY: u32 = 1 << 0;
const ERROR: u32 = 1 << 1;
const WAIT_LIMIT: u32 = 10_000_000;

#[derive(Debug, Clone, Copy)]
pub struct UpdaterMmio {
    base: *mut u32,
}

impl UpdaterMmio {
    pub const unsafe fn new() -> Self {
        Self { base: UPDATER_PERIPH_BASE as *mut u32 }
    }

    fn read(&self, offset: usize) -> u32 {
        unsafe { read_volatile(self.base.byte_add(offset)) }
    }

    fn write(&mut self, offset: usize, value: u32) {
        unsafe { write_volatile(self.base.byte_add(offset), value) }
    }

    fn wait_ready(&self, offset: usize) -> Result<(), IoError> {
        let mut remaining = WAIT_LIMIT;
        while remaining != 0 {
            let status = self.read(offset);
            if status & ERROR != 0 {
                return Err(IoError::Hardware);
            }
            if status & BUSY == 0 {
                return Ok(());
            }
            remaining -= 1;
        }
        Err(IoError::Timeout)
    }

    fn read_buffer(&self, offset: usize, out: &mut [u8]) {
        let words = out.len().div_ceil(4);
        for word_index in 0..words {
            let word = self.read(offset + word_index * 4).to_le_bytes();
            let byte_index = word_index * 4;
            let copy_len = core::cmp::min(4, out.len() - byte_index);
            out[byte_index..byte_index + copy_len].copy_from_slice(&word[..copy_len]);
        }
    }

    fn write_buffer(&mut self, offset: usize, data: &[u8]) {
        let words = data.len().div_ceil(4);
        for word_index in 0..words {
            let byte_index = word_index * 4;
            let mut word = [0xFFu8; 4];
            let copy_len = core::cmp::min(4, data.len() - byte_index);
            word[..copy_len].copy_from_slice(&data[byte_index..byte_index + copy_len]);
            self.write(offset + word_index * 4, u32::from_le_bytes(word));
        }
    }
}

impl BoardIo for UpdaterMmio {
    fn status(&self) -> u32 {
        self.read(REG_STATUS)
    }

    fn set_state(&mut self, state: u32) {
        self.write(REG_STATE, state);
    }

    fn sd_set_cs(&mut self, asserted: bool) {
        self.write(REG_SD_CONTROL, if asserted { SD_CONTROL_CS_ASSERT } else { 0 });
    }

    fn sd_set_clock_div(&mut self, half_period_cycles: u8) {
        self.write(REG_SD_CLK_DIV, half_period_cycles as u32);
    }

    fn sd_transfer_byte(&mut self, byte: u8) -> Result<u8, IoError> {
        let cs = self.read(REG_SD_CONTROL) & SD_CONTROL_CS_ASSERT;
        self.write(REG_SD_TX, byte as u32);
        self.write(REG_SD_CONTROL, cs | SD_CONTROL_START);
        self.wait_ready(REG_SD_STATUS)?;
        Ok(self.read(REG_SD_RX) as u8)
    }

    fn sd_card_detect(&self) -> bool {
        // 極性は基板依存 (CD 挿入時 LOW 前提。逆なら反転)
        self.read(REG_STATUS) & STATUS_SD_CARD_DETECT == 0
    }

    fn flash_erase_64k(&mut self, address: u32) -> Result<(), IoError> {
        self.write(REG_FLASH_ADDRESS, address);
        self.write(REG_FLASH_COMMAND, FLASH_COMMAND_ERASE_64K);
        self.wait_ready(REG_FLASH_STATUS)
    }

    fn flash_program_page(&mut self, address: u32, data: &[u8]) -> Result<(), IoError> {
        if !tang_primer_25k_sd_updater::w25q64::page_program_len_ok(address, data.len() as u32) {
            return Err(IoError::OutOfRange);
        }
        self.write_buffer(REG_FLASH_BUFFER, data);
        self.write(REG_FLASH_ADDRESS, address);
        self.write(REG_FLASH_LENGTH, data.len() as u32);
        self.write(REG_FLASH_COMMAND, FLASH_COMMAND_PROGRAM_PAGE);
        self.wait_ready(REG_FLASH_STATUS)
    }

    fn flash_read(&mut self, address: u32, out: &mut [u8]) -> Result<(), IoError> {
        if out.len() > tang_primer_25k_sd_updater::w25q64::PAGE_SIZE as usize {
            return Err(IoError::OutOfRange); // 最大 256 bytes
        }
        self.write(REG_FLASH_ADDRESS, address);
        self.write(REG_FLASH_LENGTH, out.len() as u32);
        self.write(REG_FLASH_COMMAND, FLASH_COMMAND_READ);
        self.wait_ready(REG_FLASH_STATUS)?;
        self.read_buffer(REG_FLASH_BUFFER, out);
        Ok(())
    }

    fn flash_jedec_id(&mut self) -> Result<[u8; 3], IoError> {
        self.write(REG_FLASH_COMMAND, FLASH_COMMAND_READ_JEDEC_ID);
        self.wait_ready(REG_FLASH_STATUS)?;
        Ok(self.read(REG_FLASH_JEDEC_ID).to_le_bytes()[0..3]
            .try_into()
            .unwrap_or([0; 3]))
    }

    fn set_reconfig_trigger(&mut self, asserted_low: bool) {
        self.write(REG_RECONFIG_CONTROL, if asserted_low { RECONFIG_CONTROL_ASSERT_LOW } else { 0 });
    }
}
```

---

## 6. top.veryl (完全な配線例)

PicoRV32 / rst_bridge / PicoMemBus / PicoTcm / UpdaterRegs はライブラリの
`UpdaterCore` が内蔵しているので、`top.veryl` は UpdaterCore とボード固有のピンを
配線するだけになる。SD / Flash のピンは基板に合わせて接続する。
Tang Primer 25K では Flash を CFG/MSPI ピン (E6=MCS_N, E7=CCLK, D6=MOSI, E5=MISO)、
SD を Pmod MicroSD に接続する。

```veryl
module UpdaterTop (
    clk       : input  clock           ,
    rst       : input  reset_async_high,
    sd_cs_n   : output logic           ,
    sd_sclk   : output logic           ,
    sd_mosi   : output logic           ,
    sd_miso   : input  logic           ,
    sd_cd     : input  logic           ,
    flash_cs_n: output logic           ,
    flash_sclk: output logic           ,
    flash_mosi: output logic           ,
    flash_miso: input  logic           ,
    state        : output logic<4>,  /// firmware 状態表示コード (0x0-0xF)
    reconfig_trig_n: output logic,   /// MultiBoot トリガ (外部で RECONFIG_N へ)
) {
    var state_enum: tang_primer_25k_sd_updater::updater_pkg::UpdaterState;
    inst core: tang_primer_25k_sd_updater::UpdaterCore #(
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
}
```

### パラメータの意味

| パラメータ | 値の決め方 |
| --- | --- |
| `FLASH_APP_BASE/END` | app slot の範囲 (update_spec.toml の `flash.app_base` / `+app_size`) |

`UpdaterCore` 内部の固定値:

- TCM: 32KB (ADDR_WIDTH 15)。firmware の `memory.x` (RAM 28K + STACK 4K) と一致。
  現状の updater firmware は約 20.3KB のため 32KB 必要
- `rst_bridge.DELAY_CYCLES` = 1024 (20.48μs @ 50MHz)。Gowin BSRAM が reset 後に
  使えるようになるまでの待ち。文書化されていない仕様のため余裕を持たせた値で、
  動作クロックが変わった場合の振る舞いは未定義とする
- `UpdaterRegs.BASE` = 0x03_0000 (firmware の `UPDATER_PERIPH_BASE` の下位 22bit と一致)
- TCM の hex ファイル名 = `"updater.hex"` (Gowin プロジェクトの `src/` に配置し、
  ファイルリストに追加する。詳細は 8.3 章)

### 6.1 ピン制約 (CST)

`top.veryl` のポートをピンに割り当てる CST の最小例 (Tang Primer 25K の実例)。
ピン番号は基板に合わせて変更する:

```text
// SD (Pmod MicroSD, SPI mode)
IO_LOC "sd_cs_n" F5;
IO_PORT "sd_cs_n" IO_TYPE=LVCMOS33 PULL_MODE=UP DRIVE=8 BANK_VCCIO=3.3;
IO_LOC "sd_mosi" G7;
IO_PORT "sd_mosi" IO_TYPE=LVCMOS33 PULL_MODE=UP DRIVE=8 BANK_VCCIO=3.3;
IO_LOC "sd_miso" H8;
IO_PORT "sd_miso" IO_TYPE=LVCMOS33 PULL_MODE=UP DRIVE=OFF BANK_VCCIO=3.3;
IO_LOC "sd_sclk" H5;
IO_PORT "sd_sclk" IO_TYPE=LVCMOS33 PULL_MODE=DOWN DRIVE=8 BANK_VCCIO=3.3;
IO_LOC "sd_cd" H7;
IO_PORT "sd_cd" IO_TYPE=LVCMOS33 PULL_MODE=UP DRIVE=OFF BANK_VCCIO=3.3;

// Configuration Flash (MSPI ピンを user logic で使う)
IO_LOC "flash_cs_n" E6;
IO_LOC "flash_sclk" E7;
IO_LOC "flash_mosi" D6;
IO_LOC "flash_miso" E5;

// MultiBoot トリガ (RECONFIG_N と外部ショート。外部プルアップ前提のため OPEN_DRAIN)
IO_LOC "reconfig_trig_n" A1;
IO_PORT "reconfig_trig_n" IO_TYPE=LVCMOS33 OPEN_DRAIN=ON;

// state は 4 ビットを個別ピンに割り当てる
IO_LOC "state[0]" J10;
IO_LOC "state[1]" J11;
IO_LOC "state[2]" F6;
IO_LOC "state[3]" F7;
```

`UpdaterCore` が公開する `o_state` (enum `UpdaterState`) は firmware の進行状態 (9.1 章)。
LED 表示にする場合は、利用プロジェクト側で state を加工する (例: 更新中は点滅、エラーは常灯)。

---

## 7. PicoRV32 の入手とパラメータ

- ソース: `rtl/vendor/picorv32/picorv32.v` に同梱 (ISC license、リビジョン固定)。
  別途入手の必要はない
- `picorv32.v` は Gowin プロジェクトのファイルリストに**別途追加**する
  (`veryl build` の生成物には含まれない。リポジトリ内の `rtl/vendor/picorv32/picorv32.v` を参照する)
- パラメータは `UpdaterCore` に内蔵されており固定 (CSR 不使用、`ENABLE_MUL/DIV` は firmware の
  除算・乗算に必要)。調整が必要なのは `TWO_STAGE_SHIFT / TWO_CYCLE_ALU` のみ
  (Fmax とのトレードオフ)。調整する場合はライブラリを fork / vendor して編集する
  (レジストリ取得版の内部は編集できない)

---

## 8. ビルドフロー

### 8.1 firmware.hex の生成

```sh
# 事前準備 (一度だけ)
rustup target add riscv32imc-unknown-none-elf
cargo install cargo-binutils    # cargo objcopy 用
rustup component add llvm-tools-preview
# bin2mem は https://github.com/AkiyukiOkayasu/bin2mem (cargo install --git で導入)

# ビルド
cargo build --release
cargo objcopy --release --target riscv32imc-unknown-none-elf -- -O binary updater.bin
bin2mem updater.bin updater.hex   # TCM の $readmemh 用
# updater.hex を Gowin プロジェクトの src/ にコピー (PicoTcm の HEX_FILE の解決先、8.3 章参照)
```

- `cargo build` は `.cargo/config.toml` の target 設定により riscv32imc 向けにビルドされる

### 8.2 RTL ビルド

```sh
cd <Veryl_FPGAUpdater プロジェクト>
veryl build --out-dir <Gowin プロジェクトの generated ディレクトリ>
```

生成物 (`*.sv` + `.f` ファイルリスト) を Gowin プロジェクトが参照する。

### 8.3 Gowin 合成 (MultiBoot 設定)

Gowin プロジェクトのファイルリストには、生成 SV と `picorv32.v` に加えて
`updater.hex` を追加する (`PicoTcm.HEX_FILE` の `$readmemh` が参照する。プロジェクトの
`src/updater.hex` に配置する)。`run_gowin_updater.tcl` の要点:

```tcl
set_option -top_module fpga_updater_UpdaterTop
# 起動 flash を user logic から使うための設定
set_option -use_cpu_as_gpio 1
set_option -use_mspi_as_gpio 1
# MultiBoot: 電源投入時は 0x000000、reconfig 時は 0x100000 の app を起動
set_option -multi_boot 1
set_option -multiboot_mode single
set_option -mspi_jump 0
set_option -multiboot_address_width 24
set_option -multiboot_spi_flash_address 100000
# 更新ファイル書き込みのための background programming
set_option -bg_programming userlogic
```

- `multiboot_spi_flash_address` は update_spec.toml の `flash.app_base` と一致させる
- `hotboot` / `MSPI_JUMP` は使わない

初回書き込みは factory イメージ (`make-factory-image updater.bin app.bin FACTORY.bin`)
を生成し、Gowin Programmer で Configuration Flash に `FACTORY.bin` を書き込む。
0x000000 に updater、0x100000 に app が配置されるので、以降の更新は SD カードで行える。

---

## 9. デバッグ / ブリングアップ

### 9.1 state コード表

`state[3:0]` (4bit) が firmware の進行状態を示す。trap 時は 0xF。

| 値 | 意味 | 値 | 意味 |
| --- | --- | --- | --- |
| 0x0 | Idle / reset | 0x9 | HeaderRead |
| 0x1 | SdInit | 0xA | HeaderValid |
| 0x2 | SdPowerWait | 0xB | PayloadVerify |
| 0x3 | SdDummyClock | 0xC | FlashJedec |
| 0x4 | SdCmd0 | 0xD | **AppSlotSkip (書き込みスキップ)** |
| 0x5-0x7 | (未使用) | 0xE | FlashProgram / Verify / VerifyOk |
| 0x8 | FatMounted / FileSearch | 0xF | エラー / trap |

> 注: 0x5-0x7 は割り当ての無いコード。

- **skip パス**: 0xA → 0xD → 0xE (~数秒)
- **書き込みパス**: 0xA → 0xB → 0xC → 0xE (数十秒)

### 9.2 RECONFIG_N 配線 (重要)

- `reconfig_trig_n` を **基板の `RECONFIG_N` に外部ショート**する (open-drain 出力)
- **`RECONFIG_N` ネットには外部プルアップが必須** (基板に無い場合は 10kΩ → 3.3V を追加。
  実機で見つかった落とし穴: プルアップなしだと起動時に reconfig が暴発して updater が起動しない)

### 9.3 ロジアナ観測

- `state[3:0]` と reconfig トリガ (`reconfig_trig_n` をそのまま観測) を観測する。
  SD / Flash SPI の複製出力は持たないため、SPI を観測したい場合は基板上の配線を直接
  プローブする
- skip 判定は flash の **0x03 READ** が連続する (書き込みの 0x02/0xD8 と区別すること)

### 9.4 よくある問題

| 症状 | 原因 |
| --- | --- |
| updater が起動しない | RECONFIG_N のプルアップ不足 / A1 (reconfig_trig_n ピン) のショート配線 |
| 起動が遅い (数十秒) | SD カードの応答遅延 (カード交換で改善) |
| 更新が毎回走る | app slot の内容が SD パッケージと異なる (Gowin のビットストリーム .fs を直接書いた後など) |
| 0xF で止まる | header / target / payload / flash のエラー (コード表参照) |
| 別製品のファームが入る | magic / target / layout のチェックが通る値を作成している |
