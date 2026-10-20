# CHANGELOG

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
