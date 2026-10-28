# CHANGELOG

## 0.4.0 (2026-10-28)

- Rust crates 0.4.0 / Veryl RTL 0.3.1
- `UpdateSpec.file_name` が FAT32 の LFN に対応。8.3 に収まる名前は従来どおり
  short name、収まらない名前は `open_long_name_file_in_dir` で開く
- `update_spec.toml` の `file_name` は最大 255 文字の ASCII 英数字とドットを受け付ける
- 既存の 8.3 名を使うプロジェクトは変更不要

## 0.3.1 (2026-10-27)

- Rust crates 0.3.1 / Veryl RTL 0.3.1
- flash 書き込み開始後の SD エラーで app へ reconfig しないよう修正。erase 後は
  app slot が消えている可能性があるため updater に留まる
- erase 前に SD カードと更新ファイルを開き、カード/ファイルのオープン失敗で
  app slot を消さないよう修正
- 書き込み時の SD 再読み出しデータを CRC32 で再検証
- `UpdateStatus::FlashVerifyError` を追加。readback 内容の不一致を `FlashError` と区別する
  (downstream の網羅 match は更新が必要)
- `UpdaterCore` の reset 解除遅延を 1024 cycle から 256 cycle (5.12us @ 50MHz) へ戻す
- 既知の制約: `flash_write_started` は RAM 上だけなので、erase 後の停電・リセットで
  更新中断を永続的に識別できない。metadata / Golden fallback は利用プロジェクト側の
  次フェーズで実装する

## 0.3.0 (2026-10-22)

- Rust crates 0.3.0 / Veryl RTL 0.3.0
- breaking: `BoardIo::set_state` を削除し、`set_status(UpdateStatus)` を追加 (既定 no-op)
- breaking: デバッグ用の `UpdaterState` / `UpdaterCore.o_state` を削除し、プロジェクト定義の
  8bit `DISPLAY` レジスタ / `UpdaterCore.o_display` に置き換え
- `UpdaterRegs` の 0x0C は `DISPLAY` (8bit)。CPU trap 時は `0xFF`
- Flash の CS# high を 6cycle=120ns に拡大し、Flash SCLK を 25MHz→12.5MHz に低速化。
  PnR 変動に対する tSHSL/読み出しマージンを確保する (実機の更新時間差は1秒未満)
- integration guide と README を新しい表示インターフェースに更新
- tools のテストが並列実行で同一 temp ファイルを書き合って flaky だったため、ファイル名を一意にした
- 移行: 0.2.x のままのプロジェクト (oscillator など) は、リリース時に `o_state` →
  `o_display`、`BoardIo::set_state` → `set_status` の置き換えが必要

## 0.2.1 (2026-10-21)

- Rust crates 0.2.1 / Veryl RTL 0.2.2
- SD実行クロックを6.25MHz→12.5MHzへ高速化 (初期化390kHzは据え置き)
- PicoRV32のレジスタファイルを2R1Wに戻し、cpuregsを`syn_ramstyle="distributed_ram"`でFF固定 (BSRAM複製による配置依存不具合を防止)
- 依存ライブラリを更新 (toml/syn/indexmap等)
- integration guideをriscv-rt 0.18.0 / `custom-setup-interrupts` / `opt-level="s"` / memory.x (RAM 24K + STACK 8K) に更新
- 公開API・レジスタ契約に破壊的変更なし

## 0.2.0 (2026-10-07)

- crates.io に公開: コア / build 補助 / ホストツール (0.2.0)
- Veryl registry に公開: updater RTL ライブラリ (0.2.0)
- Flash の CS# high 期間不足 (write/erase/program 系 tSHSL min 50ns に対し 1cycle=20ns) を修正し、3cycle=60ns へ延長。実機の SD 更新が verify 不一致で 0xF 終端していた問題を解決
- Flash の命名を型番非依存に汎用化 (crate は breaking): モジュール `w25q64` → `spi_nor`、`WINBOND_JEDEC_ID` → `EXAMPLE_JEDEC_ID`
- PicoTcm の BSRAM 初期化に `#[allow(initial_assign)]` を追加

## 0.1.0 (2026-08-11)

- crates.io に公開: コア (tang-primer-25k-sd-updater) / build 補助 (tang-primer-25k-sd-updater-build) / ホストツール (tang-primer-25k-sd-updater-tools)
- Veryl registry に公開: updater RTL ライブラリ (tang_primer_25k_sd_updater)
- PicoRV32 を vendor 同梱 (rtl/vendor/picorv32/, ISC license)
- `UpdaterCore`: PicoRV32 + TCM + レジスタを内蔵した、top でピン配線するだけで使えるサブシステム
