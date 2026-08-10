# gowin-sd-updater

Gowin FPGA + PicoRV32 前提の SD カードファームウェア更新機能の共有実装。
SD/FAT 読み出し、update package の検証 (magic / target / CRC32)、
Flash への書き込み、reconfigトリガまでを 1 つのフローとして提供する。

**別プロジェクトへの組み込み手順は [docs/integration-guide.md](docs/integration-guide.md) を参照**
(アーキテクチャ・パッケージ形式・BoardIo 実装例・top.veryl 完全例・ビルドフロー・
ブリングアップ手順を含む)。

## 前提 (プロジェクト側の制約)

- Gowin FPGA (Veryl)、ソフトコア CPU は PicoRV32
- PicoRV32 では CSR 命令を使わない (firmware に CSR 命令を入れない)
- Veryl 設定は `clock_type=posedge` / `reset_type=sync_high`
- システムクロック 50 MHz (delay の tick 換算に使用)
- updater firmware は TCM 32KB 以内
- reconfigは MultiBoot 方式で、`RECONFIG_N` への Low pulse (hotboot / MSPI_JUMP は使わない)。
  `RECONFIG_N` ネットには外部プルアップが必要 (基板に無い場合は 10kΩ 程度を追加)

## 動作仕様

- SD カードに更新ファイルがあれば検証して app slot に書き込み、成功後に再構成して app へ移行する
- payload の整合性検証は **CRC32** (テーブル駆動)。署名なしファイルのため SHA256 は
  改ざん耐性を提供しない (第三者の firmware を許可する設計) ので使わない
- 書き込み前に **app slot の現内容の CRC32 をパッケージの payload_crc32 と比較**し、
  一致していれば書き込みをスキップして再構成のみ行う。内容ベースの比較なので、
  書き込み後の腐食や部分書き込みは必ず検出して書き直す (自己修復)
- この比較は **SD payload の読み出し・検証より先**に行う (一致時は SD payload を
  読まずに skip するため、SD カードの応答が遅い環境でも起動が速い)
- skip 判定は header の digest を信頼する前提 (header の magic / target / layout 検証は
  実施済み)。ランダムな腐食・書き込みミスは 2^-32 程度でしか偽陽性にならず実質検出されるが、
  「flash の現在ハッシュを知って header を偽造する」意図的な改ざんは検出しない
  (第三者の firmware を許可する設計のため対象外)
- SD カードが挿入されていない場合はカード検出ピン (STATUS bit0) で即判定し、
  SD 初期化リトライを待たずに再構成して app へ移行する (カード検出の極性は
  `BoardIo::sd_card_detect` の実装側で解釈する)
- SD カードはあるが更新ファイルが無い / カードが読めない場合は、SD 探索失敗後に
  そのまま再構成して app へ移行する
- 更新ファイルがあるが検証・書き込みに失敗した場合は updater に留まる (エラー状態を報告)

## 開発環境の前提

- Rust (riscv32imc target)、Cargo
- Veryl 0.20 系 (`rtl/` のビルド)
- Verilator + make + C++ コンパイラ (`just rtl-check` の Verilator テスト)
- `just` (コマンドレシピ)

## 構成

```text
crates/
├── sd-updater/        # no_std コア。BoardIo / Updater / UpdateSpec / SD・Flash・package 処理
├── sd-updater-build/  # build.rs 補助 + spec パーサの唯一実装 (update_spec.conf → 定数 / Spec)
└── sd-updater-tools/  # ホスト CLI。update package (FPGAOSC.UPD) / factory image の生成
rtl/
├── src/               # updater RTL (Veryl ライブラリ fpga_sd_updater)
│   ├── PicoMemBus     # CPU bus の TCM / peripheral 振り分け (TCM_ADDR_WIDTH / PERI_ADDR_WIDTH)
│   ├── PicoTcm        # firmware 実行用 TCM (ADDR_WIDTH / HEX_FILE)
│   ├── rst_bridge     # 同期リセットブリッジ (DELAY_CYCLES)
│   ├── SpiByteEngine  # SPI mode 0 byte 転送エンジン
│   └── UpdaterRegs    # MMIO register block (BASE / FLASH_APP_BASE / FLASH_APP_END)
└── tests/             # Verilator cpp テスト (updater_regs / spi_byte_engine)
```

前提: RTL は path 依存で参照し、`Veryl.lock` が相対パスを記録するため、
共有 repo を `~/Documents/AkiyukiProjects/gowin-sd-updater` に配置する前提とする。

## 統合手順 (プロジェクト側)

1. `update_spec.conf` をプロジェクトに置く (`crates/sd-updater-build` のテスト参照)。
   値の基準は常にこのファイル。hw_id / flash layout はプロジェクト固有。
2. `Cargo.toml` に追加:

```toml
[dependencies]
sd_updater = { path = "../gowin-sd-updater/crates/sd-updater" }

[build-dependencies]
sd-updater-build = { path = "../gowin-sd-updater/crates/sd-updater-build" }
```

   生成コードは `sd_updater::UpdateSpec` を参照するため、依存 crate の名前は
   `sd_updater` で固定する。

3. `build.rs`:

```rust
fn main() {
    println!("cargo:rerun-if-changed=update_spec.conf");
    sd_updater_build::generate("update_spec.conf").expect("update_spec.conf を読み込めない");
}
```

4. 生成された SPEC を取り込み、`BoardIo` を実装して起動する:

```rust
include!(concat!(env!("OUT_DIR"), "/update_spec.rs"));

let mut updater = sd_updater::Updater::new(MyBoardIo::new(), SPEC);
loop {
    let status = updater.poll_once();
    updater.report_status(status);
}
```

`BoardIo` の実装例は各プロジェクトの `mmio.rs` を参照
(SD byte SPI / Flash erase・program・read / JEDEC ID / reconfig トリガを MMIO に接続する)。

5. ホストツール (`sd-updater-tools`) で update package / factory image を生成する:

```sh
cargo run -p sd-updater-tools -- make-update-package app.bin FPGAOSC.UPD --spec update_spec.conf
cargo run -p sd-updater-tools -- make-factory-image updater.bin app.bin FACTORY.bin --spec update_spec.conf
```

## RTL 統合手順 (Veryl)

1. プロジェクトの `Veryl.toml` に依存を追加 (共有 repo は
   `~/Documents/AkiyukiProjects/gowin-sd-updater` に配置する前提):

```toml
[dependencies]
fpga_sd_updater = { path = "../../../../../../gowin-sd-updater/rtl" }
```

   パスはプロジェクトの `Veryl.toml` 位置から共有 repo までの階層数で調整する。

2. `top.veryl` で共有モジュールを inst し、パラメータを明示的に渡す:

```veryl
inst regs: fpga_sd_updater::UpdaterRegs #(
    BASE          : 32'h03_0000,
    FLASH_APP_BASE: 32'h0010_0000,
    FLASH_APP_END : 32'h0020_0000,
) ( ... );

inst tcm: fpga_sd_updater::PicoTcm #(
    ADDR_WIDTH: 15,
    HEX_FILE  : "updater.hex",
) ( ... );
```

   `BASE` / `FLASH_APP_*` / TCM サイズ・hex 名はプロジェクトの flash layout と
   firmware (`mmio.rs`) に合わせる。MMIO 契約は `updaterRegs.veryl` の doc comment
   (レジスタマップ) と firmware の `mmio.rs` を突き合わせる。

3. `$readmemh` の hex ファイルは tool ごとに解決先が異なるため、生成される
   `dependencies/fpga_sd_updater/src/` を含む複数箇所に配置して合成で検証する。

## リリース同期

format_version / flash layout を共有側で変更した場合は、各プロジェクトの
`update_spec.conf` を更新する。package を作り直すまで古い spec との互換は
保持すること (更新ファイルの header format は後方互換を前提とする)。

## 開発

```bash
just check    # Rust fmt/clippy/test + rtl-check 一式
just rtl-check # rtl/ のみ: Veryl fmt/build + Verilator lint + Verilator cpp テスト
just fmt      # cargo fmt + rtl/ の veryl fmt
```

## ライセンス

MIT OR Apache-2.0 (LICENSE-MIT / LICENSE-APACHE 参照)
