#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <vector>

#include "Vtang_primer_25k_sd_updater_UpdaterRegs.h"
#include "verilated.h"

static vluint64_t main_time = 0;

double sc_time_stamp() { return static_cast<double>(main_time); }

static constexpr uint32_t BASE = 0x03'0000;
static constexpr uint32_t REG_STATUS = BASE + 0x0000;
static constexpr uint32_t REG_STATE = BASE + 0x000c;
static constexpr uint32_t REG_SD_CONTROL = BASE + 0x0010;
static constexpr uint32_t REG_SD_STATUS = BASE + 0x0014;
static constexpr uint32_t REG_SD_CLK_DIV = BASE + 0x0018;
static constexpr uint32_t REG_SD_TX = BASE + 0x001c;
static constexpr uint32_t REG_SD_RX = BASE + 0x0020;
static constexpr uint32_t REG_FLASH_ADDRESS = BASE + 0x0030;
static constexpr uint32_t REG_FLASH_LENGTH = BASE + 0x0034;
static constexpr uint32_t REG_FLASH_COMMAND = BASE + 0x0038;
static constexpr uint32_t REG_FLASH_STATUS = BASE + 0x003c;
static constexpr uint32_t REG_FLASH_JEDEC_ID = BASE + 0x0040;
static constexpr uint32_t REG_FLASH_BUFFER = BASE + 0x0300;

static void tick(Vtang_primer_25k_sd_updater_UpdaterRegs &top) {
    top.i_clk = 0;
    top.eval();
    main_time++;
    top.i_clk = 1;
    top.eval();
    main_time++;
}

static void reset(Vtang_primer_25k_sd_updater_UpdaterRegs &top) {
    top.i_rst = 1;
    top.i_mem_valid = 0;
    top.i_mem_addr = 0;
    top.i_mem_wdata = 0;
    top.i_mem_wstrb = 0;
    tick(top);
    tick(top);
    top.i_rst = 0;
    tick(top);
}

static void write_reg(Vtang_primer_25k_sd_updater_UpdaterRegs &top, uint32_t addr, uint32_t value) {
    top.i_mem_valid = 1;
    top.i_mem_addr = addr;
    top.i_mem_wdata = value;
    top.i_mem_wstrb = 0xf;
    tick(top);
    top.i_mem_valid = 0;
    top.i_mem_wstrb = 0;
    tick(top);
}

static uint32_t read_reg(Vtang_primer_25k_sd_updater_UpdaterRegs &top, uint32_t addr) {
    top.i_mem_valid = 1;
    top.i_mem_addr = addr;
    top.i_mem_wdata = 0;
    top.i_mem_wstrb = 0;
    tick(top);
    uint32_t value = top.o_mem_rdata;
    top.i_mem_valid = 0;
    tick(top);
    return value;
}

static uint32_t peek_reg(Vtang_primer_25k_sd_updater_UpdaterRegs &top, uint32_t addr) {
    top.i_mem_valid = 1;
    top.i_mem_addr = addr;
    top.i_mem_wdata = 0;
    top.i_mem_wstrb = 0;
    tick(top);
    top.eval();
    return top.o_mem_rdata;
}

static bool expect_eq(const char *name, uint32_t actual, uint32_t expected) {
    if (actual != expected) {
        std::printf("[FAIL] %s actual=0x%08x expected=0x%08x\n", name, actual, expected);
        return false;
    }
    return true;
}

static uint32_t read_le32(const std::vector<uint8_t> &bytes, size_t offset) {
    return static_cast<uint32_t>(bytes[offset]) |
           (static_cast<uint32_t>(bytes[offset + 1]) << 8) |
           (static_cast<uint32_t>(bytes[offset + 2]) << 16) |
           (static_cast<uint32_t>(bytes[offset + 3]) << 24);
}

class SpiNorModel {
public:
    SpiNorModel() : memory(8 * 1024 * 1024, 0xff) {}

    void drive(Vtang_primer_25k_sd_updater_UpdaterRegs &top) {
        static constexpr uint8_t jedec[3] = {0xef, 0x40, 0x17};
        if (top.o_flash_cs_n) {
            reset_transfer();
            top.i_flash_miso = 1;
            prev_sclk = false;
            return;
        }

        if (command_active && command == 0x9f && response_byte_index < 3) {
            const uint8_t value = jedec[response_byte_index];
            top.i_flash_miso = (value >> (7 - response_bit_index)) & 1;
        } else if (command_active && command == 0x05) {
            const uint8_t value = busy_polls_remaining == 0 ? 0x00 : 0x01;
            top.i_flash_miso = (value >> (7 - response_bit_index)) & 1;
        } else if (command_active && command == 0x03 && reading && address + response_byte_index < memory.size()) {
            const uint8_t value = memory[address + response_byte_index];
            top.i_flash_miso = (value >> (7 - response_bit_index)) & 1;
        } else {
            top.i_flash_miso = 1;
        }
    }

    void capture(Vtang_primer_25k_sd_updater_UpdaterRegs &top) {
        const bool sclk = top.o_flash_sclk != 0;
        if (!top.o_flash_cs_n && !prev_sclk && sclk) {
            rx_shift = static_cast<uint8_t>((rx_shift << 1) | (top.o_flash_mosi ? 1 : 0));
            mosi_bit_index++;
            if (mosi_bit_index == 8) {
                if (mosi_byte_index == 0) {
                    command = rx_shift;
                    command_active = true;
                    if (command == 0x06) {
                        write_enable_latch = true;
                    } else if (command == 0x05) {
                        status_poll_seen = true;
                    }
                } else if (command == 0xd8 || command == 0x02 || command == 0x03) {
                    if (mosi_byte_index <= 3) {
                        address = (address << 8) | rx_shift;
                        if (mosi_byte_index == 3) {
                            if (command == 0xd8 && write_enable_latch && address + BLOCK_SIZE <= memory.size()) {
                                std::fill(memory.begin() + address, memory.begin() + address + BLOCK_SIZE, 0xff);
                                write_enable_latch = false;
                                busy_polls_remaining = std::max<uint8_t>(busy_polls_remaining, 3);
                            } else if (command == 0x02 && write_enable_latch) {
                                programming = true;
                                program_index = 0;
                            } else if (command == 0x03) {
                                reading = true;
                            }
                        }
                    } else if (programming && address + program_index < memory.size()) {
                        memory[address + program_index] &= rx_shift;
                        program_index++;
                    }
                }
                mosi_byte_index++;
                mosi_bit_index = 0;
                rx_shift = 0;
            }
        }
        if (!top.o_flash_cs_n && prev_sclk && !sclk && response_enabled()) {
            if (response_armed) {
                if (response_bit_index == 7) {
                    response_bit_index = 0;
                    response_byte_index++;
                } else {
                    response_bit_index++;
                }
            } else {
                response_armed = true;
            }
        }
        prev_sclk = sclk;
    }

    uint8_t command = 0;
    std::vector<uint8_t> memory;
    uint8_t busy_polls_remaining = 0;
    uint32_t status_reads = 0;
    uint32_t status_zero_reads = 0;

private:
    static constexpr uint32_t BLOCK_SIZE = 64 * 1024;

    bool response_enabled() const {
        return command_active &&
               ((command == 0x9f && response_byte_index < 3) ||
               command == 0x05 ||
               (command == 0x03 && reading));
    }

    void reset_transfer() {
        if (command == 0x02 && programming && program_index != 0) {
            write_enable_latch = false;
            busy_polls_remaining = std::max<uint8_t>(busy_polls_remaining, 2);
        } else if (status_poll_seen && busy_polls_remaining != 0) {
            busy_polls_remaining--;
            status_reads++;
        } else if (status_poll_seen) {
            status_zero_reads++;
        }
        status_poll_seen = false;
        mosi_bit_index = 0;
        mosi_byte_index = 0;
        response_bit_index = 0;
        response_byte_index = 0;
        response_armed = false;
        command_active = false;
        rx_shift = 0;
        address = 0;
        programming = false;
        reading = false;
        program_index = 0;
    }

    bool prev_sclk = false;
    uint8_t mosi_bit_index = 0;
    uint16_t mosi_byte_index = 0;  // 256B ページ + 4B コマンド/アドレスで 8bit を超える
    uint8_t response_bit_index = 0;
    uint8_t response_byte_index = 0;
    bool response_armed = false;
    bool command_active = false;
    uint8_t rx_shift = 0;
    size_t address = 0;
    size_t program_index = 0;
    bool write_enable_latch = false;
    bool programming = false;
    bool reading = false;
    bool status_poll_seen = false;
};

static void tick_flash(Vtang_primer_25k_sd_updater_UpdaterRegs &top, SpiNorModel &flash) {
    top.i_clk = 0;
    top.eval();
    flash.drive(top);
    top.eval();
    main_time++;
    top.i_clk = 1;
    top.eval();
    flash.drive(top);
    top.eval();
    flash.capture(top);
    main_time++;
}

static void write_reg_flash(
    Vtang_primer_25k_sd_updater_UpdaterRegs &top,
    SpiNorModel &flash,
    uint32_t addr,
    uint32_t value
) {
    top.i_mem_valid = 1;
    top.i_mem_addr = addr;
    top.i_mem_wdata = value;
    top.i_mem_wstrb = 0xf;
    tick_flash(top, flash);
    top.i_mem_valid = 0;
    top.i_mem_wstrb = 0;
    tick_flash(top, flash);
}

static uint32_t read_reg_flash(Vtang_primer_25k_sd_updater_UpdaterRegs &top, SpiNorModel &flash, uint32_t addr) {
    top.i_mem_valid = 1;
    top.i_mem_addr = addr;
    top.i_mem_wdata = 0;
    top.i_mem_wstrb = 0;
    tick_flash(top, flash);
    uint32_t value = top.o_mem_rdata;
    top.i_mem_valid = 0;
    tick_flash(top, flash);
    return value;
}

static bool wait_flash_ready(Vtang_primer_25k_sd_updater_UpdaterRegs &top, SpiNorModel &flash) {
    for (int i = 0; i < 8000; i++) {
        if ((read_reg_flash(top, flash, REG_FLASH_STATUS) & 1) == 0) {
            return true;
        }
    }
    std::printf(
        "[FAIL] flash wait timeout status=0x%08x command=0x%02x status_reads=%u zero_reads=%u busy_left=%u\n",
        read_reg_flash(top, flash, REG_FLASH_STATUS),
        flash.command,
        flash.status_reads,
        flash.status_zero_reads,
        flash.busy_polls_remaining
    );
    return false;
}

static bool wait_sd_ready(Vtang_primer_25k_sd_updater_UpdaterRegs &top) {
    for (int i = 0; i < 2000; i++) {
        if ((read_reg(top, REG_SD_STATUS) & 1) == 0) {
            return true;
        }
    }
    return false;
}

int main(int argc, char **argv) {
    Verilated::commandArgs(argc, argv);

    Vtang_primer_25k_sd_updater_UpdaterRegs top;
    top.i_flash_miso = 1;
    top.i_sd_miso = 1;
    reset(top);
    SpiNorModel flash;

    bool ok = true;
    ok &= expect_eq("status reset", read_reg(top, REG_STATUS), 0);
    ok &= expect_eq("sd control reset", read_reg(top, REG_SD_CONTROL), 0);
    ok &= expect_eq("sd status reset", read_reg(top, REG_SD_STATUS), 0);
    ok &= expect_eq("sd clk div reset", read_reg(top, REG_SD_CLK_DIV), 64);
    ok &= expect_eq("sd tx reset", read_reg(top, REG_SD_TX), 0xff);
    ok &= expect_eq("sd rx reset", read_reg(top, REG_SD_RX), 0);
    ok &= expect_eq("flash status reset", read_reg(top, REG_FLASH_STATUS), 0);
    ok &= expect_eq("jedec id reset", read_reg(top, REG_FLASH_JEDEC_ID), 0);
    ok &= expect_eq("state reset", top.o_state, 0);

    write_reg(top, REG_STATE, 0x0000'000f);
    ok &= expect_eq("state register", read_reg(top, REG_STATE), 0x0000'000f);
    ok &= expect_eq("state output", top.o_state, 0x0000'000f);

    write_reg(top, REG_SD_CLK_DIV, 2);
    write_reg(top, REG_SD_CONTROL, 0x0000'0002);
    ok &= expect_eq("sd clk div", read_reg(top, REG_SD_CLK_DIV), 2);
    ok &= expect_eq("sd control cs", read_reg(top, REG_SD_CONTROL), 0x0000'0002);
    ok &= expect_eq("sd cs asserted", top.o_sd_cs_n, 0);
    write_reg(top, REG_SD_TX, 0x0000'00a5);
    write_reg(top, REG_SD_CONTROL, 0x0000'0003);
    ok &= expect_eq("sd control start latched as cs only", read_reg(top, REG_SD_CONTROL), 0x0000'0002);
    ok &= expect_eq("sd tx", read_reg(top, REG_SD_TX), 0x0000'00a5);
    ok &= expect_eq("sd busy set", read_reg(top, REG_SD_STATUS), 1);
    ok &= expect_eq("sd ready", wait_sd_ready(top) ? 1 : 0, 1);
    ok &= expect_eq("sd rx high", read_reg(top, REG_SD_RX), 0x0000'00ff);
    ok &= expect_eq("sd status done", read_reg(top, REG_SD_STATUS), 0);
    write_reg(top, REG_SD_CONTROL, 0);
    ok &= expect_eq("sd control deassert", read_reg(top, REG_SD_CONTROL), 0);
    ok &= expect_eq("sd cs deasserted", top.o_sd_cs_n, 1);
    write_reg(top, REG_SD_TX, 0x0000'00ff);
    write_reg(top, REG_SD_CONTROL, 0x0000'0001);
    bool dummy_sclk_high = false;
    for (int i = 0; i < 128; i++) {
        tick(top);
        dummy_sclk_high = dummy_sclk_high || (top.o_sd_sclk != 0);
    }
    ok &= expect_eq("sd dummy clocks with cs high", dummy_sclk_high ? 1 : 0, 1);
    ok &= expect_eq("sd dummy ready", wait_sd_ready(top) ? 1 : 0, 1);
    ok &= expect_eq("sd dummy cs remains high", top.o_sd_cs_n, 1);

    reset(top);

    write_reg(top, REG_FLASH_ADDRESS, 0x0010'0000);
    write_reg(top, REG_FLASH_LENGTH, 0x0000'0100);
    ok &= expect_eq("flash address", read_reg(top, REG_FLASH_ADDRESS), 0x0010'0000);
    ok &= expect_eq("flash length", read_reg(top, REG_FLASH_LENGTH), 0x0000'0100);
    write_reg_flash(top, flash, REG_FLASH_COMMAND, 0x0000'0004);
    ok &= expect_eq("flash command", read_reg_flash(top, flash, REG_FLASH_COMMAND), 0x0000'0004);
    ok &= expect_eq("flash busy set", read_reg_flash(top, flash, REG_FLASH_STATUS), 1);
    ok &= expect_eq("flash ready", wait_flash_ready(top, flash) ? 1 : 0, 1);
    ok &= expect_eq("flash command opcode", flash.command, 0x9f);
    ok &= expect_eq("jedec id", read_reg_flash(top, flash, REG_FLASH_JEDEC_ID), 0x0017'40ef);

    write_reg(top, REG_FLASH_BUFFER + 0, 0x0302'0100);
    write_reg(top, REG_FLASH_BUFFER + 4, 0x0706'0504);
    write_reg(top, REG_FLASH_ADDRESS, 0x0010'0000);
    write_reg(top, REG_FLASH_LENGTH, 0x0000'0008);
    write_reg_flash(top, flash, REG_FLASH_COMMAND, 0x0000'0002);
    ok &= expect_eq("flash program ready", wait_flash_ready(top, flash) ? 1 : 0, 1);
    const uint32_t program_status_reads = flash.status_reads;
    ok &= expect_eq("flash program polls busy", program_status_reads >= 2 ? 1 : 0, 1);
    ok &= expect_eq("flash program polls ready", flash.status_zero_reads >= 1 ? 1 : 0, 1);
    ok &= expect_eq("flash programmed word 0", read_le32(flash.memory, 0x0010'0000), 0x0302'0100);
    ok &= expect_eq("flash programmed word 1", read_le32(flash.memory, 0x0010'0004), 0x0706'0504);

    write_reg(top, REG_FLASH_BUFFER + 0, 0xffff'ffff);
    write_reg(top, REG_FLASH_BUFFER + 4, 0xffff'ffff);
    write_reg(top, REG_FLASH_ADDRESS, 0x0010'0000);
    write_reg(top, REG_FLASH_LENGTH, 0x0000'0008);
    write_reg_flash(top, flash, REG_FLASH_COMMAND, 0x0000'0003);
    ok &= expect_eq("flash read ready", wait_flash_ready(top, flash) ? 1 : 0, 1);
    ok &= expect_eq("flash read word 0", read_reg_flash(top, flash, REG_FLASH_BUFFER + 0), 0x0302'0100);
    ok &= expect_eq("flash read word 1", read_reg_flash(top, flash, REG_FLASH_BUFFER + 4), 0x0706'0504);

    write_reg(top, REG_FLASH_ADDRESS, 0x0010'0000);
    write_reg_flash(top, flash, REG_FLASH_COMMAND, 0x0000'0001);
    ok &= expect_eq("flash erase ready", wait_flash_ready(top, flash) ? 1 : 0, 1);
    ok &= expect_eq("flash erase polls busy", flash.status_reads > program_status_reads ? 1 : 0, 1);
    ok &= expect_eq("flash erased word 0", read_le32(flash.memory, 0x0010'0000), 0xffff'ffff);

    write_reg(top, REG_FLASH_BUFFER + 0, 0xa3a2'a1a0);
    write_reg(top, REG_FLASH_BUFFER + 252, 0xfffe'fdfc);
    ok &= expect_eq("flash buffer word 0", read_reg(top, REG_FLASH_BUFFER + 0), 0xa3a2'a1a0);
    ok &= expect_eq("flash buffer last", read_reg(top, REG_FLASH_BUFFER + 252), 0xfffe'fdfc);

    write_reg(top, REG_FLASH_ADDRESS, 0x0000'0000);
    write_reg(top, REG_FLASH_LENGTH, 0x0000'0004);
    write_reg_flash(top, flash, REG_FLASH_COMMAND, 0x0000'0002);
    ok &= expect_eq("flash guard error", read_reg_flash(top, flash, REG_FLASH_STATUS), 2);
    ok &= expect_eq("flash guard preserved updater", read_le32(flash.memory, 0), 0xffff'ffff);

    if (!ok) {
        std::printf("DONE (FAIL)\n");
        return 1;
    }
    std::printf("DONE (PASS)\n");
    return 0;
}
