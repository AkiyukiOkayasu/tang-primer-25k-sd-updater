# RELEASES

## 0.1.0 (未リリース)

- SD updater コア (sd-updater) / build 補助 (sd-updater-build) / ホストツール / updater RTL を FPGA_Oscillator から切り出し
- 公開 API (Rust): `BoardIo` / `Updater::new(io, spec)` / `UpdateSpec` / `UpdateStatus`
- 共有 RTL (Veryl `fpga_sd_updater`): `PicoMemBus` / `PicoTcm` / `rst_bridge` / `SpiByteEngine` / `UpdaterRegs` (パラメータ化)
- ホストツールは `--spec <update_spec.conf>` 必須
- Verilator テスト: `just rtl-check` (updater_regs / spi_byte_engine)
