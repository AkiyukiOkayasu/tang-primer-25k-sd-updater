check: rtl-check
    @echo "🎨 フォーマットチェック..."
    cargo fmt --check
    @echo "🧪 clippy..."
    cargo clippy --all-targets -- -D warnings
    @echo "🧪 テスト..."
    cargo test
    @echo "🔍 Python tools の構文チェック..."
    python3 -m py_compile tools/update_spec.py
    python3 -m py_compile tools/make_update_package/make_update_package.py
    python3 -m py_compile tools/make_factory_image/make_factory_image.py
    @echo "✅ check 完了"

fmt:
    cargo fmt
    (cd rtl && veryl fmt)

# 共有 updater RTL (rtl/) の fmt / build / lint / Verilator テスト
[working-directory('rtl')]
rtl-check:
    @echo "🎨 Veryl フォーマットチェック..."
    veryl fmt --check
    @echo "🔨 Veryl RTL をビルド..."
    veryl build
    @echo "🔎 Verilator lint..."
    verilator -Wall --lint-only -sv --top-module fpga_sd_updater_UpdaterRegs \
        --Wno-fatal -f fpga_sd_updater.f
    verilator -Wall --lint-only -sv --top-module fpga_sd_updater_SpiByteEngine \
        --Wno-fatal -f fpga_sd_updater.f
    @echo "🧪 register block Verilator test..."
    rm -rf tests/updater_regs_obj_dir
    mkdir -p tests/updater_regs_obj_dir
    verilator -Wall -sv --cc --exe --top-module fpga_sd_updater_UpdaterRegs \
        --Wno-fatal \
        -Mdir tests/updater_regs_obj_dir \
        -CFLAGS "-std=c++17 -O2" \
        target/spiByteEngine.sv \
        target/updaterRegs.sv \
        tests/updater_regs.cpp
    make -C tests/updater_regs_obj_dir -f Vfpga_sd_updater_UpdaterRegs.mk -j
    tests/updater_regs_obj_dir/Vfpga_sd_updater_UpdaterRegs
    @echo "🧪 SPI byte engine Verilator test..."
    rm -rf tests/spi_byte_engine_obj_dir
    mkdir -p tests/spi_byte_engine_obj_dir
    verilator -Wall -sv --cc --exe --top-module fpga_sd_updater_SpiByteEngine \
        --Wno-fatal \
        -Mdir tests/spi_byte_engine_obj_dir \
        -CFLAGS "-std=c++17 -O2" \
        target/spiByteEngine.sv \
        tests/spi_byte_engine.cpp
    make -C tests/spi_byte_engine_obj_dir -f Vfpga_sd_updater_SpiByteEngine.mk -j
    tests/spi_byte_engine_obj_dir/Vfpga_sd_updater_SpiByteEngine
    @echo "✅ rtl-check 完了"
