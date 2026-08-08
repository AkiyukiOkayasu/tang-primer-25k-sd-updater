# RELEASES

## 0.1.0 (未リリース)

- SD updater コア (sd-updater) / build 補助 (sd-updater-build) / ホストツールを FPGA_Oscillator から切り出し
- 公開 API: `BoardIo` / `Updater::new(io, spec)` / `UpdateSpec` / `UpdateStatus`
- ホストツールは `--spec <update_spec.conf>` 必須
