#include <cstdint>
#include <cstdio>
#include <vector>

#include "Vfpga_sd_updater_SpiByteEngine.h"
#include "verilated.h"

static vluint64_t main_time = 0;

double sc_time_stamp() { return static_cast<double>(main_time); }

static void eval(Vfpga_sd_updater_SpiByteEngine &top) {
    top.eval();
    main_time++;
}

static void tick(Vfpga_sd_updater_SpiByteEngine &top) {
    top.i_clk = 0;
    eval(top);
    top.i_clk = 1;
    eval(top);
}

static void reset(Vfpga_sd_updater_SpiByteEngine &top) {
    top.i_rst = 1;
    top.i_start = 0;
    top.i_tx_data = 0;
    top.i_miso = 0;
    tick(top);
    tick(top);
    top.i_rst = 0;
    tick(top);
}

static bool expect_eq(const char *name, uint32_t actual, uint32_t expected) {
    if (actual != expected) {
        std::printf("[FAIL] %s actual=0x%08x expected=0x%08x\n", name, actual, expected);
        return false;
    }
    return true;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);

    Vfpga_sd_updater_SpiByteEngine top;
    reset(top);

    bool ok = true;
    ok &= expect_eq("idle busy", top.o_busy, 0);
    ok &= expect_eq("idle done", top.o_done, 0);
    ok &= expect_eq("idle sclk", top.o_sclk, 0);

    const uint8_t tx = 0xa5;
    const uint8_t rx = 0x3c;
    top.i_tx_data = tx;
    top.i_start = 1;
    tick(top);
    top.i_start = 0;

    std::vector<uint8_t> mosi_bits;
    for (int bit = 7; bit >= 0; bit--) {
        top.eval();
        ok &= expect_eq("sclk low during mosi setup", top.o_sclk, 0);
        mosi_bits.push_back(top.o_mosi ? 1 : 0);
        top.i_miso = (rx >> bit) & 1;

        tick(top);
        ok &= expect_eq("sclk high during sample phase", top.o_sclk, 1);

        tick(top);
        ok &= expect_eq("sclk low after sample", top.o_sclk, 0);
    }

    ok &= expect_eq("done seen", top.o_done, 1);
    ok &= expect_eq("rx data", top.o_rx_data, rx);
    ok &= expect_eq("busy after done", top.o_busy, 1);

    tick(top);
    ok &= expect_eq("idle after done", top.o_busy, 0);
    ok &= expect_eq("done pulse cleared", top.o_done, 0);

    ok &= expect_eq("mosi bit count", static_cast<uint32_t>(mosi_bits.size()), 8);
    uint8_t observed_tx = 0;
    for (uint8_t bit : mosi_bits) {
        observed_tx = static_cast<uint8_t>((observed_tx << 1) | bit);
    }
    ok &= expect_eq("mosi byte", observed_tx, tx);

    if (!ok) {
        std::printf("DONE (FAIL)\n");
        return 1;
    }
    std::printf("DONE (PASS)\n");
    return 0;
}
