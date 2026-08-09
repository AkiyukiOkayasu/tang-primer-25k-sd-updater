//! テスト用のメモリ上 BoardIo 実装。

use crate::{BoardIo, IoError};

#[derive(Debug, Clone)]
pub struct FakeMmio {
    pub status: u32,
    pub debug_state: u32,
    pub sd_cs_asserted: bool,
    pub sd_clock_div: u8,
    pub sd_tx: std::vec::Vec<u8>,
    pub sd_rx: std::vec::Vec<u8>,
    #[allow(dead_code)]
    pub flash: std::vec::Vec<u8>,
    #[allow(dead_code)]
    pub jedec_id: [u8; 3],
    pub fail_io: bool,
    pub flash_erase_count: u32,
    pub flash_program_count: u32,
    pub flash_read_count: u32,
    pub flash_jedec_count: u32,
    pub reconfig_asserted_low: bool,
    pub reconfig_assert_count: u32,
    pub reconfig_release_count: u32,
}

impl Default for FakeMmio {
    fn default() -> Self {
        Self {
            status: 0,
            debug_state: 0,
            sd_cs_asserted: false,
            sd_clock_div: 64,
            sd_tx: std::vec::Vec::new(),
            sd_rx: std::vec::Vec::new(),
            flash: std::vec![0xFFu8; 0x200000],
            jedec_id: [0xEF, 0x40, 0x17],
            fail_io: false,
            flash_erase_count: 0,
            flash_program_count: 0,
            flash_read_count: 0,
            flash_jedec_count: 0,
            reconfig_asserted_low: false,
            reconfig_assert_count: 0,
            reconfig_release_count: 0,
        }
    }
}

impl BoardIo for FakeMmio {
    fn status(&self) -> u32 {
        self.status
    }

    fn set_debug_state(&mut self, state: u32) {
        self.debug_state = state;
    }

    fn sd_set_cs(&mut self, asserted: bool) {
        self.sd_cs_asserted = asserted;
    }

    fn sd_set_clock_div(&mut self, half_period_cycles: u8) {
        self.sd_clock_div = half_period_cycles;
    }

    fn sd_transfer_byte(&mut self, byte: u8) -> Result<u8, IoError> {
        if self.fail_io {
            return Err(IoError::Hardware);
        }
        self.sd_tx.push(byte);
        Ok(if self.sd_rx.is_empty() {
            0xFF
        } else {
            self.sd_rx.remove(0)
        })
    }

    fn flash_erase_64k(&mut self, address: u32) -> Result<(), IoError> {
        self.flash_erase_count += 1;
        if self.fail_io {
            return Err(IoError::Hardware);
        }
        let start = address as usize;
        let end = start.checked_add(0x10000).ok_or(IoError::OutOfRange)?;
        let range = self.flash.get_mut(start..end).ok_or(IoError::OutOfRange)?;
        range.fill(0xFF);
        Ok(())
    }

    fn flash_program_page(&mut self, address: u32, data: &[u8]) -> Result<(), IoError> {
        self.flash_program_count += 1;
        if self.fail_io {
            return Err(IoError::Hardware);
        }
        let start = address as usize;
        let end = start.checked_add(data.len()).ok_or(IoError::OutOfRange)?;
        let range = self.flash.get_mut(start..end).ok_or(IoError::OutOfRange)?;
        for (dst, src) in range.iter_mut().zip(data) {
            *dst &= *src;
        }
        Ok(())
    }

    fn flash_read(&mut self, address: u32, out: &mut [u8]) -> Result<(), IoError> {
        self.flash_read_count += 1;
        if self.fail_io {
            return Err(IoError::Hardware);
        }
        let start = address as usize;
        let end = start.checked_add(out.len()).ok_or(IoError::OutOfRange)?;
        let range = self.flash.get(start..end).ok_or(IoError::OutOfRange)?;
        out.copy_from_slice(range);
        Ok(())
    }

    fn flash_jedec_id(&mut self) -> Result<[u8; 3], IoError> {
        self.flash_jedec_count += 1;
        Ok(self.jedec_id)
    }

    fn set_reconfig_trigger(&mut self, asserted_low: bool) {
        self.reconfig_asserted_low = asserted_low;
        if asserted_low {
            self.reconfig_assert_count += 1;
        } else {
            self.reconfig_release_count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Updater;
    use crate::sd_spi::MmioSdSpi;
    use crate::spec::TEST_SPEC;
    use crate::updater::UpdateStatus;
    use embedded_hal::spi::{Operation, SpiDevice};

    #[test]
    fn updater_starts_in_idle_poll_state() {
        let mmio = FakeMmio {
            fail_io: true,
            ..Default::default()
        };
        let mut updater = Updater::new(mmio, TEST_SPEC);
        assert_eq!(updater.poll_once(), UpdateStatus::FatIoError);
        assert_eq!(updater.poll_once(), UpdateStatus::FatIoError);
    }

    #[test]
    fn no_card_transitions_to_app() {
        let mmio = FakeMmio::default();
        let mut updater = Updater::new(mmio, TEST_SPEC);
        assert_eq!(updater.poll_once(), UpdateStatus::FatIoError);
        let mmio = updater.into_inner();
        assert_eq!(mmio.reconfig_assert_count, 1);
        assert_eq!(mmio.reconfig_release_count, 1);
    }

    #[test]
    fn sd_spi_transaction_controls_cs_and_transfers_bytes() {
        let mut mmio = FakeMmio {
            sd_rx: vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x12, 0x34],
            ..Default::default()
        };
        {
            let mut spi = MmioSdSpi::new(&mut mmio);
            let mut read = [0u8; 2];
            let mut ops = [
                Operation::Write(&[0x40, 0, 0, 0, 0, 0x95]),
                Operation::Read(&mut read),
            ];
            spi.transaction(&mut ops).unwrap();
            assert_eq!(read, [0x12, 0x34]);
        }

        assert!(mmio.sd_cs_asserted);
        assert_eq!(&mmio.sd_tx[..6], &[0x40, 0, 0, 0, 0, 0x95]);
        assert_eq!(&mmio.sd_tx[6..8], &[0xFF, 0xFF]);
        assert_eq!(mmio.debug_state, 0x4);
    }

    #[test]
    fn sd_idle_clocks_keep_cs_deasserted() {
        let mut mmio = FakeMmio::default();
        {
            let mut spi = MmioSdSpi::new(&mut mmio);
            spi.clock_idle_bytes(10).unwrap();
        }

        assert!(!mmio.sd_cs_asserted);
        assert_eq!(mmio.sd_tx, vec![0xFF; 10]);
    }

    #[test]
    fn reconfig_trigger_tracks_low_pulse_edges() {
        let mut mmio = FakeMmio::default();

        mmio.set_reconfig_trigger(true);
        assert!(mmio.reconfig_asserted_low);
        assert_eq!(mmio.reconfig_assert_count, 1);

        mmio.set_reconfig_trigger(false);
        assert!(!mmio.reconfig_asserted_low);
        assert_eq!(mmio.reconfig_release_count, 1);
    }
}
