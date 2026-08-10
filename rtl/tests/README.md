# RTL の Verilator テスト

このリポジトリの RTL は Veryl (`rtl/src/`) が正であり、生成 SV は直接編集しない。
`rtl/tests/` は開発者向けの機能テスト (C++ テストベンチ + Verilator) である。
エンドユーザー (利用プロジェクト) が実行を準備する必要はない。

## 前提

- `veryl build` 済みであること (`rtl/target/*.sv` が生成されている)
- verilator / make が利用可能であること

## 実行手順

### updater_regs (レジスタブロック)

```sh
verilator -Wall -sv --cc --exe --top-module tang_primer_25k_sd_updater_UpdaterRegs \
    --Wno-fatal \
    -Mdir tests/updater_regs_obj_dir \
    -CFLAGS "-std=c++17 -O2" \
    target/spiByteEngine.sv \
    target/updaterRegs.sv \
    tests/updater_regs.cpp
make -C tests/updater_regs_obj_dir -f Vtang_primer_25k_sd_updater_UpdaterRegs.mk -j
tests/updater_regs_obj_dir/Vtang_primer_25k_sd_updater_UpdaterRegs
```

### spi_byte_engine (SPI byte エンジン)

```sh
verilator -Wall -sv --cc --exe --top-module tang_primer_25k_sd_updater_SpiByteEngine \
    --Wno-fatal \
    -Mdir tests/spi_byte_engine_obj_dir \
    -CFLAGS "-std=c++17 -O2" \
    target/spiByteEngine.sv \
    tests/spi_byte_engine.cpp
make -C tests/spi_byte_engine_obj_dir -f Vtang_primer_25k_sd_updater_SpiByteEngine.mk -j
tests/spi_byte_engine_obj_dir/Vtang_primer_25k_sd_updater_SpiByteEngine
```
