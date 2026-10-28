# RELEASES

## 0.4.0 (2026-10-28)

- Rust crates 0.4.0 / Veryl RTL 0.3.1
- `file_name` の FAT32 LFN 対応。8.3 名は従来どおり short name で開き、
  8.3 に収まらない名前は long name で開く
- 既存プロジェクトは 8.3 名のままなら変更不要

## 0.3.1 (2026-10-27)

- flash 書き込み開始後の SD エラーで app へ reconfig しないよう修正。
  erase 後は app slot が消えている可能性があるため、updater に留まる
- erase 前に SD カードと更新ファイルを開き、カード/ファイルのオープン失敗で app slot を消さないよう修正
- 書き込み時の SD 再読み出しデータを CRC32 で再検証し、検証時と異なるデータを
  書き込んだ場合は PayloadError とする
- `UpdateStatus::FlashVerifyError` を追加。readback 内容の不一致を `FlashError` と
  区別できるようにする (downstream の網羅 match は更新が必要)
- `UpdaterCore` の reset 解除遅延を 1024 cycle から 256 cycle (5.12us @ 50MHz) へ戻す
- 既知の制約: `flash_write_started` は RAM 上だけなので、erase 後の停電・リセットで
  更新中断を永続的に識別できない。metadata / Golden fallback は利用プロジェクト側の
  次フェーズで実装する

## 0.3.0 (2026-10-22)

- 表示APIを変更 (breaking): `o_state` / `UpdaterState` / `BoardIo::set_state` を削除し、
  プロジェクト定義の 8bit `DISPLAY` レジスタ (`o_display`) と `BoardIo::set_status` に移行。
  enum の未定義値を firmware が書くと Gowin 合成後の比較が不正になる問題を根本的に避ける
- CPU trap 時は `o_display=0xFF`。表示コードの意味はプロジェクト側で定義する
- Flash の CS# high を 120ns に拡大し、SCLK を 12.5MHz に低速化して PnR マージンを確保。
  実機の FPGA_ADAT で SD 更新経路を確認済み (更新時間の差は1秒未満)
- tools のテストを並列実行でも安定するよう修正
- 0.2.x のプロジェクトは `o_state` → `o_display`、`set_state` → `set_status` の移行が必要

## 0.2.1 (2026-10-21)

- SD実行クロックを12.5MHzへ高速化し、PicoRV32のレジスタファイルを2R1Wへ戻す
- cpuregsのBSRAM推論を防ぐ合成属性を追加
- 依存ライブラリ更新、integration guideのriscv-rt 0.18.0 / `opt-level="s"` 追随

## 0.2.0 (2026-10-07)

- Flash 書込の CS# high 期間不足 (tSHSL 50ns 未満) を修正し、実機の SD 更新 0xF 終端を解決
- Flash 命名の型番非依存化 (`w25q64` → `spi_nor`、crate は breaking)
- PicoTcm の BSRAM 初期化 allow 追加

## 0.1.0 (2026-08-11)

- crates.io / Veryl registry への初回公開 (0.1.0)
- 機能: SD (FAT32) からの更新ファイル検証・app slot 書き換え・reconfig。書き込みスキップ (自己修復) / 自動移行
