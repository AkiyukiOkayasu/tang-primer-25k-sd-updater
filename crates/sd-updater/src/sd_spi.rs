//! SD SPI ドライバ (MMIO byte SPI を embedded-sdmmc の BlockDevice に接続)。

use core::marker::PhantomData;

use embedded_hal::delay::DelayNs;
use embedded_hal::spi::{ErrorType, Operation, SpiDevice};

use crate::{BoardIo, IoError};

/// SD 初期化時の SPI クロック分周 (half period をサイクル数で指定)。
pub const SD_INIT_HALF_PERIOD_CYCLES: u8 = 64;
/// 通常動作時の SPI クロック分周。
pub const SD_RUN_HALF_PERIOD_CYCLES: u8 = 4;
/// システムクロック。delay の tick 換算に使う。異なる周波数のプロジェクトでは書き換える。
const SYS_CLK_HZ: u32 = 50_000_000;

pub struct FirmwareDelay;

impl DelayNs for FirmwareDelay {
    fn delay_ns(&mut self, ns: u32) {
        let ticks = ns_to_ticks(ns);
        if ticks == 0 {
            return;
        }

        let start = read_cycle_counter();
        while read_cycle_counter().wrapping_sub(start) < ticks {
            core::hint::spin_loop();
        }
    }
}

fn ns_to_ticks(ns: u32) -> u32 {
    let ticks = (ns as u64 * SYS_CLK_HZ as u64).div_ceil(1_000_000_000);
    ticks as u32
}

#[cfg(target_arch = "riscv32")]
fn read_cycle_counter() -> u32 {
    let value: u32;
    unsafe {
        core::arch::asm!(
            "rdcycle {value}",
            value = out(reg) value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[cfg(not(target_arch = "riscv32"))]
fn read_cycle_counter() -> u32 {
    use core::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub struct MmioSdSpi<'a, Io: BoardIo> {
    io: *mut Io,
    command_trace: bool,
    _marker: PhantomData<&'a mut Io>,
}

impl<'a, Io: BoardIo> MmioSdSpi<'a, Io> {
    #[cfg(test)]
    pub fn new(io: &'a mut Io) -> Self {
        Self {
            io,
            command_trace: true,
            _marker: PhantomData,
        }
    }

    /// `io` の参照はトランザクションの間だけ有効でなければならない。
    /// updater は SD と Flash を同時に使わず、単一 CPU から同期アクセスする前提で使用する。
    ///
    /// # Safety
    ///
    /// 呼び出し側が `io` の生存期間と排他を保証すること。
    pub unsafe fn from_raw(io: *mut Io) -> Self {
        Self {
            io,
            command_trace: true,
            _marker: PhantomData,
        }
    }

    pub fn set_clock_div(&mut self, half_period_cycles: u8) {
        self.io_mut().sd_set_clock_div(half_period_cycles);
    }

    pub fn set_command_trace(&mut self, enabled: bool) {
        self.command_trace = enabled;
    }

    pub fn clock_idle_bytes(&mut self, count: usize) -> Result<(), IoError> {
        self.io_mut().sd_set_cs(false);
        for _ in 0..count {
            self.io_mut().sd_transfer_byte(0xFF)?;
        }
        Ok(())
    }

    fn io_mut(&mut self) -> &mut Io {
        // SdCard/VolumeManager が所有するのはこのadapterだけで、transactionは同期的に完了する。
        // updaterは同時にSDとFlashを動かさないため、MMIOアクセスの直列化は呼び出し順で保証する。
        unsafe { &mut *self.io }
    }
}

impl<Io: BoardIo> ErrorType for MmioSdSpi<'_, Io> {
    type Error = IoError;
}

impl<Io: BoardIo> SpiDevice<u8> for MmioSdSpi<'_, Io> {
    fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Self::Error> {
        // embedded-sdmmc 0.9 は command write と response polling を別々の
        // SpiDevice call に分けるため、call ごとに CS を戻すと SD command が壊れる。
        // updater の SD SPI は専用 bus なので、明示的な idle clock 時以外は選択を保持する。
        self.io_mut().sd_set_cs(true);
        self.run_operations(operations)
    }
}

impl<Io: BoardIo> MmioSdSpi<'_, Io> {
    fn run_operations(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), IoError> {
        for operation in operations {
            match operation {
                Operation::Read(buffer) => {
                    for byte in buffer.iter_mut() {
                        *byte = self.io_mut().sd_transfer_byte(0xFF)?;
                    }
                }
                Operation::Write(buffer) => {
                    for byte in buffer.iter() {
                        self.observe_command_byte(*byte);
                        let _ = self.io_mut().sd_transfer_byte(*byte)?;
                    }
                }
                Operation::Transfer(read, write) => {
                    let transfer_len = core::cmp::max(read.len(), write.len());
                    for index in 0..transfer_len {
                        let tx = write.get(index).copied().unwrap_or(0xFF);
                        self.observe_command_byte(tx);
                        let rx = self.io_mut().sd_transfer_byte(tx)?;
                        if let Some(dst) = read.get_mut(index) {
                            *dst = rx;
                        }
                    }
                }
                Operation::TransferInPlace(buffer) => {
                    for byte in buffer.iter_mut() {
                        self.observe_command_byte(*byte);
                        *byte = self.io_mut().sd_transfer_byte(*byte)?;
                    }
                }
                Operation::DelayNs(ns) => {
                    let mut delay = FirmwareDelay;
                    delay.delay_ns(*ns);
                }
            }
        }
        Ok(())
    }

    fn observe_command_byte(&mut self, byte: u8) {
        if !self.command_trace {
            return;
        }
        if byte & 0xC0 != 0x40 {
            return;
        }

        let state = match byte & 0x3F {
            0 => Some(0x4),
            8 => Some(0x5),
            55 | 41 => Some(0x6),
            58 => Some(0x7),
            17 | 18 => Some(0x8),
            _ => None,
        };
        if let Some(state) = state {
            self.io_mut().set_debug_state(state);
        }
    }
}
