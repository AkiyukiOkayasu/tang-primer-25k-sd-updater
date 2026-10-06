# RELEASES

## 0.2.0 (2026-10-07)

- Flash 書込の CS# high 期間不足 (tSHSL 50ns 未満) を修正し、実機の SD 更新 0xF 終端を解決
- Flash 命名の型番非依存化 (`w25q64` → `spi_nor`、crate は breaking)
- PicoTcm の BSRAM 初期化 allow 追加

## 0.1.0 (2026-08-11)

- crates.io / Veryl registry への初回公開 (0.1.0)
- 機能: SD (FAT32) からの更新ファイル検証・app slot 書き換え・reconfig。書き込みスキップ (自己修復) / 自動移行
