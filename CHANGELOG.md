# CHANGELOG

## 0.1.0 (未リリース)

- SD updater コア (sd-updater) / build 補助 (sd-updater-build) / ホストツール (sd-updater-tools) / updater RTL を FPGA_Oscillator から切り出し
- 公開 API (Rust): `BoardIo` / `Updater::new(io, spec)` / `UpdateSpec` / `UpdateStatus` / `Spec` / `load()`
- 共有 RTL (Veryl `fpga_sd_updater`): `PicoMemBus` / `PicoTcm` / `rst_bridge` / `SpiByteEngine` / `UpdaterRegs` (パラメータ化)
- ホストツールは Rust CLI (Python 版から置換、出力は byte 同一)
- Verilator テスト: `just rtl-check` (updater_regs / spi_byte_engine)
- SD カード無し/更新ファイル無し時は app へ自動移行 (更新エラー時は updater に留まる)
- app slot の現内容がパッケージと一致 (CRC32) する場合は書き込みをスキップ (自己修復)
