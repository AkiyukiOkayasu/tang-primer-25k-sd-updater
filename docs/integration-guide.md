# 統合ガイド (別プロジェクト向け)

このガイドは、このリポジトリ (gowin-sd-updater) を**別の Gowin FPGA プロジェクトに組み込む**
ための手順です。このリポジトリだけを見て、firmware + RTL + ビルドフローまで実装できることを
目的とします。

前提: **GW5A 系 (Arora V) FPGA** + **PicoRV32 ソフトコア** + **Veryl**。
再構成は Gowin MultiBoot (`RECONFIG_N` への Low pulse) を使用します。

---

## 1. アーキテクチャ概要

```text
                    ┌─────────────────────────────────────────────┐
                    │ FPGA (GW5A 系)                               │
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
                    │ オンボード SPI Flash (W25Q64 等)      │
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
|---|---|---|
| firmware コア | `crates/sd-updater` (no_std) | `BoardIo` 実装 + `main.rs` + `update_spec.conf` |
| ビルド補助 | `crates/sd-updater-build` | `build.rs` から呼ぶ |
| ホストツール | `crates/sd-updater-tools` | なし (CLI として使用) |
| RTL ライブラリ | `rtl/` (Veryl `fpga_sd_updater`) | `top.veryl` (配線のみ) |
| Verilator テスト | `rtl/tests/` | なし |

---

## 2. 更新フロー (firmware の動作)

`Updater::poll_once()` が 1 回呼ばれると、以下のフローが最後まで進む:

1. `open_sd_card()` — SD カードを初期化
   - まず `sd_card_detect()` で未挿入を即判定 (未挿入なら即エラー → app へ移行)
   - 100ms 待機 → dummy clock → CMD0/CMD8/ACMD41/CMD58 (embedded-sdmmc)
   - FAT ボリュームをマウント
2. `read_header()` — 固定ファイル名 (`UpdateSpec.file_name`) の header を読み、検証:
   - magic / format_version / target_hw_id / target_fpga_id / flash_layout_id / payload_size
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

## 3. パッケージ形式 (FPGAOSC.UPD)

ホストツール `sd-updater-tools make-update-package` が生成する形式。
header (0x58 = 88 bytes) + payload (app bitstream) の連結。

| offset | サイズ | フィールド | 説明 |
|---|---|---|---|
| 0x00 | 8 | magic | ファイル形式識別子 (プロジェクト固有) |
| 0x08 | 4 | format_version | パッケージ形式バージョン (現行 1) |
| 0x0C | 4 | target_hw_id | 製品識別子 (プロジェクト固有) |
| 0x10 | 4 | target_fpga_id | 対象 FPGA 識別子 |
| 0x14 | 4 | flash_layout_id | フラッシュ配置識別子 (プロジェクト固有) |
| 0x18 | 4 | app_version | アプリバージョン (情報のみ、判定に使わない) |
| 0x1C | 4 | payload_offset | payload のオフセット (= header_size) |
| 0x20 | 4 | payload_size | payload のサイズ |
| 0x24 | 4 | payload_crc32 | payload の CRC32 |
| 0x28 | 32 | reserved | 旧 payload_sha256 フィールド (ゼロ埋め) |

値の基準は常にプロジェクトの `update_spec.conf`。

- `header_size` は **0x48 以上**であること (sha256 フィールドが収まる範囲。
  0x58 の場合 0x48..0x58 の 16 バイトはゼロ埋めの予約領域)
- チェックの詳細: magic / format_version / target / layout は header のみで判定。
  payload の整合性は CRC32 (書き込み前 + skip 判定)

---

## 4. プロジェクトへの追加手順

> 配置前提: 共有 repo は `~/Documents/AkiyukiProjects/gowin-sd-updater` に置き、
> プロジェクトから相対パスで参照する (以下はプロジェクトを `Firmware/<プロジェクト名>`
> の下に置いた場合のパス。階層が違う場合は調整)。

### 4.1 Cargo (firmware)

`Cargo.toml`:

```toml
[package]
name = "rv32updater"
version = "0.1.0"
edition = "2024"

[dependencies]
sd_updater = { package = "sd-updater", path = "../gowin-sd-updater/crates/sd-updater" }

[build-dependencies]
sd-updater-build = { path = "../gowin-sd-updater/crates/sd-updater-build" }

[target.'cfg(target_arch = "riscv32")'.dependencies]
panic-halt = "1.0.0"
riscv-rt = { version = "0.17.1", features = ["memory", "single-hart", "no-mhartid", "no-xie-xip", "no-xtvec"] }
```

- `edition = "2024"` が必要 (`#[unsafe(export_name = ...)]` 構文のため)
- `update_spec.conf` は 4.3 で定義する (build.rs がビルド時に参照)

`.cargo/config.toml` (riscv ターゲット固定):

```toml
[build]
target = "riscv32imc-unknown-none-elf"

[target.riscv32imc-unknown-none-elf]
rustflags = ["-C", "link-arg=-Tlink.x"]
runner = "false # RISC-V binary cannot run on host."
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

- RAM+STACK の合計が `PicoTcm.ADDR_WIDTH` の容量と一致すること (32KB = 15bit)
- `build.rs` で memory.x を OUT_DIR へコピーし、`cargo:rustc-link-search` を出す

`build.rs`:

```rust
fn main() {
    println!("cargo:rerun-if-changed=update_spec.conf");
    sd_updater_build::generate("update_spec.conf").expect("update_spec.conf を読み込めない");
}
```

`main.rs` (entry。`mmio` モジュールは第 5 節のテンプレートで実装する):

```rust
#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

mod mmio;

#[cfg(not(test))]
use panic_halt as _; // panic 時のハンドラ (リンクに必要)

use mmio::UpdaterMmio;
use sd_updater::Updater;

include!(concat!(env!("OUT_DIR"), "/update_spec.rs"));

#[cfg(not(test))]
#[unsafe(export_name = "_setup_interrupts")]
fn setup_interrupts() {}

#[cfg_attr(not(test), riscv_rt::entry)]
fn main() -> ! {
    let mmio = unsafe { UpdaterMmio::new() };
    let mut updater = Updater::new(mmio, SPEC);
    loop {
        let status = updater.poll_once();
        updater.report_status(status);
    }
}
```

- `include!` で生成された `SPEC` を使う (生成コードは `sd_updater::UpdateSpec` を参照するため、
  依存 crate の名前は `sd_updater` で固定)
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
fpga_sd_updater = { path = "../../../../../../gowin-sd-updater/rtl" }
```

(パスはプロジェクトの `Veryl.toml` 位置から共有 repo までの階層数で調整)

### 4.3 update_spec.conf

プロジェクト固有の値 (hw_id / flash layout) を定義する唯一のファイル。
キー集合と構文は `crates/sd-updater-build/src/lib.rs` の `SAMPLE_CONF`
(テスト内) を参照。

```text
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
```

各キーの役割:

| キー | 役割 |
|---|---|
| `package.*` | パッケージ形式の識別子 (第 3 節の表) |
| `flash.updater_base/size` | updater bitstream を置く領域 (通常 0x000000 から) |
| `flash.app_base/size` | 更新対象の app slot。firmware の書き込み先と `UpdaterRegs.FLASH_APP_*` に一致させる |
| `flash.metadata_*` | 将来の適用済み管理用に予約 (現行フローでは未使用) |
| `flash.golden_updater_*` | 将来の Golden fallback 用に予約 (現行フローでは未使用) |
| `flash.layout_id` | レイアウト識別子 (第 2 章参照) |

---

## 5. BoardIo 実装 (テンプレート)

`BoardIo` は SD byte SPI / Flash 操作 / reconfig トリガを MMIO レジスタに接続する。
レジスタ配置は RTL の `rtl/src/updaterRegs.veryl` の doc comment
(MMIO 契約表) と一致させること。

```rust
use core::ptr::{read_volatile, write_volatile};
use sd_updater::{BoardIo, IoError};

pub const UPDATER_PERIPH_BASE: usize = 0x0043_0000; // PicoMemBus の peripheral 窓 + BASE

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
const REG_FLASH_COMMAND: usize = 0x38; // 1=ERASE_64K 2=PROGRAM_PAGE 3=READ 4=JEDEC
const REG_FLASH_STATUS: usize = 0x3C;  // bit0 BUSY / bit1 ERROR
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
        // 極性は基板依存 (この実装では Pmod の CD が挿入時 LOW の前提)。
        // 実機で逆ならここを反転する。
        self.read(REG_STATUS) & STATUS_SD_CARD_DETECT == 0
    }

    fn flash_erase_64k(&mut self, address: u32) -> Result<(), IoError> {
        self.write(REG_FLASH_ADDRESS, address);
        self.write(REG_FLASH_COMMAND, FLASH_COMMAND_ERASE_64K);
        self.wait_ready(REG_FLASH_STATUS)
    }

    fn flash_program_page(&mut self, address: u32, data: &[u8]) -> Result<(), IoError> {
        if !sd_updater::w25q64::page_program_len_ok(address, data.len() as u32) {
            return Err(IoError::OutOfRange);
        }
        self.write_buffer(REG_FLASH_BUFFER, data);
        self.write(REG_FLASH_ADDRESS, address);
        self.write(REG_FLASH_LENGTH, data.len() as u32);
        self.write(REG_FLASH_COMMAND, FLASH_COMMAND_PROGRAM_PAGE);
        self.wait_ready(REG_FLASH_STATUS)
    }

    fn flash_read(&mut self, address: u32, out: &mut [u8]) -> Result<(), IoError> {
        if out.len() > sd_updater::w25q64::PAGE_SIZE as usize {
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

`top.veryl` が作るもの: PicoRV32 (SV) + `rst_bridge` + `PicoMemBus` + `PicoTcm` +
`UpdaterRegs` + デバッグ出力の配線。SD / Flash のピンは基板に合わせて接続する。

```veryl
module UpdaterTop (
    clk       : input  clock           , /// システムクロック
    rst       : input  reset_async_high, /// リセット
    sd_cs_n   : output logic           , /// SD CS#
    sd_sclk   : output logic           , /// SD SCK
    sd_mosi   : output logic           , /// SD MOSI
    sd_miso   : input  logic           , /// SD MISO
    sd_cd     : input  logic           , /// SD card detect
    flash_cs_n: output logic           , /// Flash CS#
    flash_sclk: output logic           , /// Flash SCK
    flash_mosi: output logic           , /// Flash MOSI
    flash_miso: input  logic           , /// Flash MISO
    state : output logic<4>,        /// firmware 状態表示コード (0x0-0xF)
    reconfig_trig_n: output logic,      /// MultiBoot トリガ (外部で RECONFIG_N へ)
) {
    var rst_delayed: reset_sync_high;
    inst reset_bridge: fpga_sd_updater::rst_bridge #(
        DELAY_CYCLES: 256,
    ) ( clk, rst, rst_sync: rst_delayed );

    var mem_valid: logic;    var mem_ready: logic;
    var mem_addr : logic<32>; var mem_wdata: logic<32>;
    var mem_wstrb: logic<4>; var mem_rdata: logic<32>;
    var trap: logic;

    inst rv: $sv::picorv32 #(
        LATCHED_MEM_RDATA   : 1            ,
        TWO_STAGE_SHIFT     : 1            ,
        BARREL_SHIFTER      : 0            ,
        TWO_CYCLE_COMPARE   : 1            ,
        TWO_CYCLE_ALU       : 1            ,
        COMPRESSED_ISA      : 1            ,
        CATCH_MISALIGN      : 1            ,
        CATCH_ILLINSN       : 1            ,
        DISABLE_CSR         : 1            ,
        ENABLE_PCPI         : 0            ,
        ENABLE_MUL          : 1            ,
        ENABLE_DIV          : 1            ,
        ENABLE_IRQ          : 0            ,
        REGS_INIT_ZERO      : 1            ,
        PROGADDR_RESET      : 32'h0000_0000,
        STACKADDR           : 32'h0000_8000,
    ) (
        clk       : clk          ,
        resetn    : ~rst_delayed ,
        trap      : trap         ,
        mem_valid : mem_valid    ,
        mem_instr : _            ,
        mem_ready : mem_ready    ,
        mem_addr  : mem_addr     ,
        mem_wdata : mem_wdata    ,
        mem_wstrb : mem_wstrb    ,
        mem_rdata : mem_rdata    ,
        pcpi_wr   : 0            ,
        pcpi_wait : 0            ,
        pcpi_ready: 0            ,
        pcpi_rd   : 0            ,
        irq       : 32'h0000_0000,
    );

    var tcm_mem_valid: logic;    var tcm_mem_addr : logic<15>;
    var tcm_mem_wdata: logic<32>; var tcm_mem_wstrb: logic<4>;
    var tcm_mem_rdata: logic<32>;
    var peri_mem_valid: logic;    var peri_mem_addr : logic<22>;
    var peri_mem_wdata: logic<32>; var peri_mem_wstrb: logic<4>;
    var peri_mem_rdata: logic<32>;

    inst mem_bus: fpga_sd_updater::PicoMemBus (
        i_clk: clk, i_rst: rst_delayed,
        i_mem_valid: mem_valid, i_mem_addr: mem_addr,
        i_mem_wdata: mem_wdata, i_mem_wstrb: mem_wstrb,
        o_mem_ready: mem_ready, o_mem_rdata: mem_rdata,
        o_tcm_valid: tcm_mem_valid, o_tcm_addr: tcm_mem_addr,
        o_tcm_wdata: tcm_mem_wdata, o_tcm_wstrb: tcm_mem_wstrb,
        i_tcm_rdata: tcm_mem_rdata,
        o_peri_valid: peri_mem_valid, o_peri_addr: peri_mem_addr,
        o_peri_wdata: peri_mem_wdata, o_peri_wstrb: peri_mem_wstrb,
        i_peri_rdata: peri_mem_rdata,
    );

    inst tcm: fpga_sd_updater::PicoTcm #(
        ADDR_WIDTH: 15,          // 32KB (firmware の memory.x と合わせる)
        HEX_FILE  : "updater.hex",
    ) (
        i_clk: clk,
        i_mem_valid: tcm_mem_valid, i_mem_addr: tcm_mem_addr,
        i_mem_wdata: tcm_mem_wdata, i_mem_wstrb: tcm_mem_wstrb,
        o_mem_rdata: tcm_mem_rdata,
    );

    var state_enum: fpga_sd_updater::updater_pkg::UpdaterState;
    var state_out : logic<4>;
    inst regs: fpga_sd_updater::UpdaterRegs #(
        BASE          : 32'h03_0000,   // firmware の UPDATER_PERIPH_BASE の下位 22bit
        FLASH_APP_BASE: 32'h0010_0000, // update_spec.conf の app_base と一致させる
        FLASH_APP_END : 32'h0020_0000, // app_base + app_size
    ) (
        i_clk: clk, i_rst: rst_delayed,
        i_mem_valid: peri_mem_valid, i_mem_addr: peri_mem_addr,
        i_mem_wdata: peri_mem_wdata, i_mem_wstrb: peri_mem_wstrb,
        o_mem_rdata: peri_mem_rdata,
        o_state: state_enum,
        o_sd_cs_n: sd_cs_n, o_sd_sclk: sd_sclk, o_sd_mosi: sd_mosi,
        i_sd_miso: sd_miso, i_sd_cd: sd_cd,
        o_flash_cs_n: flash_cs_n, o_flash_sclk: flash_sclk,
        o_flash_mosi: flash_mosi, i_flash_miso: flash_miso,
        o_reconfig_trig_n: reconfig_trig_n,
    );

    assign state_out = state_enum;              // enum → logic (暗黙変換)
    assign state     = if trap ? 4'hF : state_out;
}
```

### パラメータの意味

| パラメータ | 値の決め方 |
|---|---|
| `PicoTcm.ADDR_WIDTH` | TCM サイズ。firmware の `memory.x` の RAM+STACK 合計と一致させる (32KB = 15) |
| `PicoTcm.HEX_FILE` | `$readmemh` のファイル名。Gowin 合成時の解決先は tool 依存なので、生成される `dependencies/fpga_sd_updater/src/` を含む複数箇所に hex を配置して検証する |
| `UpdaterRegs.BASE` | peripheral 窓内のベースオフセット (firmware の `UPDATER_PERIPH_BASE` の下位 22bit) |
| `UpdaterRegs.FLASH_APP_BASE/END` | app slot の範囲 (update_spec.conf の `flash.app_base` / `+app_size`) |

---

## 7. PicoRV32 の入手とパラメータ

- ソース: `picorv32.v` (cliffordwolf/PicoRV32 の `picorv32.v` をプロジェクトに取り込む。
  **リビジョンは固定する** — リビジョンによりポート構成が変わるため、このガイドの
  接続例は固定リビジョン前提)
- updater 用の推奨パラメータ (第 6 節の例):
  - `DISABLE_CSR: 1` — CSR 命令を使わない前提
  - `ENABLE_PCPI: 0` / `ENABLE_IRQ: 0` — 未使用機能は無効化 (デフォルトは 1 のため明示する)
  - `PROGADDR_RESET: 32'h0000_0000` — TCM の先頭から起動
  - `STACKADDR: 32'h0000_8000` — firmware の memory.x の STACK と一致
  - `TWO_STAGE_SHIFT / TWO_CYCLE_ALU` — Fmax とのトレードオフ (タイミングに問題があれば調整)
  - `ENABLE_MUL / ENABLE_DIV: 1` — firmware の除算・乗算に必要 (embedded-sdmmc が使う)
- `picorv32.v` は Gowin プロジェクトのファイルリストに**別途追加**する
  (`veryl build` の生成物には含まれない)

---

## 8. ビルドフロー

### 8.1 firmware.hex の生成

```sh
# 事前準備 (一度だけ)
rustup target add riscv32imc-unknown-none-elf
cargo install cargo-binutils    # cargo objcopy 用
rustup component add llvm-tools-preview
# bin2mem はプロジェクトに合わせて用意 (cargo-binutils の objcopy -O verilog 等で代替可)

# ビルド
cargo build --release
cargo objcopy --release --target riscv32imc-unknown-none-elf -- -O binary updater.bin
bin2mem updater.bin updater.hex   # TCM の $readmemh 用
# updater.hex を RTL プロジェクトの参照先にコピー (PicoTcm の HEX_FILE の解決先)
```

- TCM サイズ制約: `updater.bin` が `memory.x` の RAM+STACK 合計 (32KB) 以内であること
- `cargo build` は `.cargo/config.toml` の target 設定により riscv32imc 向けにビルドされる

### 8.2 RTL ビルド

```sh
cd <Veryl_FPGAUpdater プロジェクト>
veryl build --out-dir <Gowin プロジェクトの generated ディレクトリ>
```

生成物 (`*.sv` + `.f` ファイルリスト) を Gowin プロジェクトが参照する。

### 8.3 Gowin 合成 (MultiBoot 設定)

`run_gowin_updater.tcl` の要点:

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

- `multiboot_spi_flash_address` は update_spec.conf の `flash.app_base` と一致させる
- `hotboot` / `MSPI_JUMP` は使わない

---

## 9. デバッグ / ブリングアップ

### 9.1 state コード表

`state[3:0]` (4bit) が firmware の進行状態を示す。trap 時は 0xF。

| 値 | 意味 | 値 | 意味 |
|---|---|---|---|
| 0x0 | Idle / reset | 0x9 | HeaderRead |
| 0x1 | SdInit | 0xA | HeaderValid |
| 0x2 | SdPowerWait | 0xB | PayloadVerify |
| 0x3 | SdDummyClock | 0xC | FlashJedec |
| 0x4 | SdCmd0 | 0xD | **AppSlotSkip (書き込みスキップ)** |
| 0x5-0x7 | (未使用) | 0xE | FlashProgram / Verify / VerifyOk |
| 0x8 | FatMounted / FileSearch | 0xF | エラー / trap |

> 注: この表は enum のコード割り当て。実際に report されるのは
> 0x1 → 0x2 → 0x3 → 0x4 → 0x8 → 0x9 → 0xA → (0xB / 0xC) → (0xD / 0xE) の順で、
> 0x5-0x7 は割り当ての無いコード。

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
- ブリングアップは LED よりロジアナ優先
- skip 判定は flash の **0x03 READ** が連続する (書き込みの 0x02/0xD8 と区別すること)

### 9.4 よくある問題

| 症状 | 原因 |
|---|---|
| updater が起動しない | RECONFIG_N のプルアップ不足 / A1 等のショート配線 |
| 起動が遅い (数十秒) | SD カードの応答遅延 (カード交換で改善) |
| 更新が毎回走る | app slot の内容が SD パッケージと異なる (`program-flash` 等で .fs を書いた後など) |
| 0xF で止まる | header / target / payload / flash のエラー (コード表参照) |
| 別製品のファームが入る | magic / target / layout のチェックが通る値を作成している (意図的な場合のみ) |

---

## 10. 前提条件のまとめ

- GW5A 系 (Arora V) FPGA (MultiBoot + MSPI-as-GPIO)
- PicoRV32 ソフトコア (CSR 不使用)
- Veryl: `clock_type=posedge` / `reset_type=sync_high`
- システムクロック 50 MHz (firmware の delay 換算)
- updater firmware は TCM 32KB 以内
- SD カード (FAT32)、固定ファイル名の更新ファイル
- 再構成は MultiBoot + `RECONFIG_N` Low pulse (hotboot / MSPI_JUMP 不使用)
