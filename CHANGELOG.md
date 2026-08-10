# CHANGELOG

## 0.1.0 (2026-08-11)

- crates.io に公開: コア (tang-primer-25k-sd-updater) / build 補助 (tang-primer-25k-sd-updater-build) / ホストツール (tang-primer-25k-sd-updater-tools)
- Veryl registry に公開: updater RTL ライブラリ (tang_primer_25k_sd_updater)
- PicoRV32 を vendor 同梱 (rtl/vendor/picorv32/, ISC license)
- `UpdaterCore`: PicoRV32 + TCM + レジスタを内蔵した、top でピン配線するだけで使えるサブシステム
