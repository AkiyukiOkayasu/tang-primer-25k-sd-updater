//! SD カードから package を読み、検証して app slot へ書き込む更新フロー本体。

use crate::sd_spi::{
    FirmwareDelay, MmioSdSpi, SD_INIT_HALF_PERIOD_CYCLES, SD_RUN_HALF_PERIOD_CYCLES,
};
use crate::{BoardIo, IoError, UpdateSpec, crc32, package, spi_nor};
use embedded_hal::delay::DelayNs;
use embedded_sdmmc::{BlockDevice, Mode, SdCard, TimeSource, Timestamp, VolumeIdx, VolumeManager};

const SD_DUMMY_CLOCK_BYTES: usize = 256;
const RECONFIG_PULSE_MS: u32 = 1;

/// 更新フローの進行状態。`state_code()` で 4bit の状態表示コードに変換され、
/// RTL の `o_state` 出力 (top の `state` ポート) に表示される。
/// コード値は RTL の `updater_pkg::UpdaterState` と対応する (変更時は両者を同期)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStatus {
    /// 初期状態 / 更新なし (正常終了のフォールバック)。コード 0x0。
    Idle,
    /// SD 初期化開始。コード 0x1。
    SdInit,
    /// SD 電源安定待ち (100ms)。コード 0x2。
    SdPowerWait,
    /// SD ダミークロック送出。コード 0x3。
    SdDummyClock,
    /// SD CMD0 (SPI mode 移行)。コード 0x4。
    SdCmd0,
    /// FAT ボリュームマウント完了。コード 0x8。
    FatMounted,
    /// 更新ファイル探索中。コード 0x8。
    FileSearch,
    /// header 読み出し中。コード 0x9。
    HeaderRead,
    /// header の target / layout 検証完了。コード 0xA。
    HeaderValid,
    /// SD payload の CRC32 検証中 (書き込みパスのみ)。コード 0xB。
    PayloadVerify,
    /// Flash JEDEC ID 確認 (書き込みパスのみ)。コード 0xC。
    FlashJedec,
    /// Flash 書き込み中 (ページ単位)。コード 0xE。
    FlashProgram,
    /// Flash readback verify 中。コード 0xE。
    FlashVerify,
    /// 更新成功 (FlashVerifyOk)。reconfig で app へ移行する。コード 0xE。
    FlashVerifyOk,
    /// app slot の現内容がパッケージと一致し、書き込みをスキップした。コード 0xD。
    AppSlotSkip,
    /// SD I/O 失敗 (カード無し・応答なし)。この状態では app へ移行する。コード 0xF。
    FatIoError,
    /// 更新ファイルが見つからない。app へ移行する。コード 0xF。
    FileNotFound,
    /// FAT 形式エラー (カード読めず)。app へ移行する。コード 0xF。
    FatFormatError,
    /// header 検証エラー (magic / format 等)。updater に留まる。コード 0xF。
    HeaderError,
    /// target (hw / fpga) 不一致。updater に留まる。コード 0xF。
    TargetError,
    /// payload 検証エラー (CRC32 不一致・slot 範囲外)。updater に留まる。コード 0xF。
    PayloadError,
    /// Flash 操作エラー。updater に留まる。コード 0xF。
    FlashError,
}

impl UpdateStatus {
    fn state_code(self) -> u32 {
        match self {
            UpdateStatus::Idle => 0x0,
            UpdateStatus::SdInit => 0x1,
            UpdateStatus::SdPowerWait => 0x2,
            UpdateStatus::SdDummyClock => 0x3,
            UpdateStatus::SdCmd0 => 0x4,
            UpdateStatus::FatMounted | UpdateStatus::FileSearch => 0x8,
            UpdateStatus::HeaderRead => 0x9,
            UpdateStatus::HeaderValid => 0xA,
            UpdateStatus::PayloadVerify => 0xB,
            UpdateStatus::FlashJedec => 0xC,
            // 0xD は書き込みスキップの判定表示に割り当てる (erase は report しない)。
            UpdateStatus::AppSlotSkip => 0xD,
            UpdateStatus::FlashProgram
            | UpdateStatus::FlashVerify
            | UpdateStatus::FlashVerifyOk => 0xE,
            UpdateStatus::FatIoError
            | UpdateStatus::FileNotFound
            | UpdateStatus::FatFormatError
            | UpdateStatus::HeaderError
            | UpdateStatus::TargetError
            | UpdateStatus::PayloadError
            | UpdateStatus::FlashError => 0xF,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateError {
    SdIo,
    SdFileNotFound,
    SdFormat,
    Header(package::HeaderError),
    WrongTarget,
    WrongLayout,
    PayloadCrc,
    SlotRange,
    Flash(IoError),
    FlashJedec,
    VerifyMismatch,
}

pub struct Updater<Io> {
    io: Io,
    spec: UpdateSpec,
    terminal_status: Option<UpdateStatus>,
    page: [u8; spi_nor::PAGE_SIZE as usize],
    verify: [u8; spi_nor::PAGE_SIZE as usize],
}

impl<Io: BoardIo + 'static> Updater<Io> {
    pub const fn new(io: Io, spec: UpdateSpec) -> Self {
        Self {
            io,
            spec,
            terminal_status: None,
            page: [0; spi_nor::PAGE_SIZE as usize],
            verify: [0; spi_nor::PAGE_SIZE as usize],
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn into_inner(self) -> Io {
        self.io
    }

    /// 1 回呼ぶごとに更新フローを最後まで進める。終端状態に達した後は同じ状態を返す。
    ///
    /// SD カードが無い・更新ファイルが無い・カードが読めない場合は、そのまま通常 app へ
    /// reconfigして移行する。app slot が空/破損なら config 失敗で次回電源投入時に
    /// 先頭 updater へ戻る (Golden fallback 未使用のため)。
    pub fn poll_once(&mut self) -> UpdateStatus {
        if let Some(status) = self.terminal_status {
            return status;
        }

        self.report_status(UpdateStatus::SdInit);
        let status = self.run_update().map_or_else(map_update_error, |updated| {
            if updated {
                UpdateStatus::FlashVerifyOk
            } else {
                UpdateStatus::Idle
            }
        });
        self.io.sd_set_cs(false);
        self.terminal_status = Some(status);

        if matches!(
            status,
            UpdateStatus::FatIoError | UpdateStatus::FileNotFound | UpdateStatus::FatFormatError
        ) {
            self.trigger_app_reconfig();
        }

        status
    }

    pub fn report_status(&mut self, status: UpdateStatus) {
        self.io.set_state(status.state_code());
    }

    fn run_update(&mut self) -> Result<bool, UpdateError> {
        let mut card = self.open_sd_card()?;
        self.report_status(UpdateStatus::FatMounted);
        self.report_status(UpdateStatus::FileSearch);

        self.report_status(UpdateStatus::HeaderRead);
        let header = self.read_header(&mut card)?;
        self.validate_target(header)?;
        self.validate_layout(header)?;
        drop(card);
        self.io.sd_set_cs(false);

        // app slot の現内容がパッケージと一致していれば書き込みをスキップする。
        // 内容ベースの比較なので、書き込み後の腐食・部分書き込みは必ず検出して書き直す (自己修復)。
        // 一致時は SD payload の読み出し・検証をせずに skip する (SD カードの応答が遅い
        // 環境ではこれが起動時間の大半を占めるため)。flash が header の digest と一致する
        // ことは flash 内容の正当性を直接示すので、SD payload の検証は不要。
        self.report_status(UpdateStatus::HeaderValid);
        if self.app_slot_matches(header)? {
            self.report_status(UpdateStatus::AppSlotSkip);
            self.report_status(UpdateStatus::FlashVerifyOk);
            self.trigger_app_reconfig();
            return Ok(true);
        }

        let mut card = self.open_sd_card()?;
        self.report_status(UpdateStatus::PayloadVerify);
        self.validate_payload_digest(&mut card, header)?;
        drop(card);
        self.io.sd_set_cs(false);

        self.program_app_slot(header)?;
        self.report_status(UpdateStatus::FlashVerifyOk);
        self.trigger_app_reconfig();
        Ok(true)
    }

    /// app slot の現内容の CRC32 がパッケージの payload_crc32 と一致するか。
    /// 一致 = 書き込み不要 (現内容が正しい)。不一致 = 書き込みが必要。
    ///
    /// CRC32 はランダムな腐食・書き込みミスの検出には十分 (2^-32) で、PicoRV32 では
    /// SHA256 より一桁以上速い (テーブル駆動)。意図的な改ざんは対象外。
    fn app_slot_matches(&mut self, header: package::UpdateHeader) -> Result<bool, UpdateError> {
        let mut crc = crc32::Crc32::new();
        let mut remaining = header.payload_size;
        let mut address = self.spec.app_base;
        while remaining != 0 {
            let chunk_len = core::cmp::min(remaining, self.page.len() as u32) as usize;
            self.io
                .flash_read(address, &mut self.page[..chunk_len])
                .map_err(UpdateError::Flash)?;
            crc.update(&self.page[..chunk_len]);
            remaining -= chunk_len as u32;
            address += chunk_len as u32;
        }
        Ok(crc.finish() == header.payload_crc32)
    }

    fn open_sd_card(&mut self) -> Result<UpdaterSdCard<Io>, UpdateError>
    where
        Io: 'static,
    {
        self.report_status(UpdateStatus::SdPowerWait);
        // カード検出ピンで即判定する。未挿入なら embedded-sdmmc の CMD0/ACMD41
        // リトライ待ち (数十秒) をせずに、すぐ app 移行の経路へ進む。
        if !self.io.sd_card_detect() {
            return Err(UpdateError::SdIo);
        }
        let mut delay = FirmwareDelay;
        // SD仕様的には1ms待機すれば十分だが、カードによっては起動に時間がかかるものもあるようなので、余裕を持って100ms待つ
        delay.delay_ms(100);

        let io = &mut self.io as *mut Io;
        // MMIOは単一CPUから同期アクセスする。SD adapterはFlash操作と同時に使わない。
        let mut spi: MmioSdSpi<'static, Io> = unsafe { MmioSdSpi::from_raw(io) };
        spi.set_clock_div(SD_INIT_HALF_PERIOD_CYCLES);
        self.report_status(UpdateStatus::SdDummyClock);
        spi.clock_idle_bytes(SD_DUMMY_CLOCK_BYTES)
            .map_err(|_| UpdateError::SdIo)?;

        let sdcard = SdCard::new(spi, FirmwareDelay);
        self.report_status(UpdateStatus::SdCmd0);
        sdcard.num_blocks().map_err(|_| UpdateError::SdIo)?;
        sdcard.spi(|spi| {
            spi.set_clock_div(SD_RUN_HALF_PERIOD_CYCLES);
        });
        Ok(VolumeManager::new(sdcard, NullTime))
    }

    fn read_header(
        &mut self,
        card: &mut UpdaterSdCard<Io>,
    ) -> Result<package::UpdateHeader, UpdateError> {
        let mut header_bytes = [0u8; crate::MAX_HEADER_SIZE];
        let volume = card.open_volume(VolumeIdx(0)).map_err(map_sd_error)?;
        let root = volume.open_root_dir().map_err(map_sd_error)?;
        let file = root
            .open_file_in_dir(self.spec.file_name, Mode::ReadOnly)
            .map_err(map_sd_error)?;
        read_exact(&file, &mut header_bytes)?;
        package::UpdateHeader::parse(&header_bytes, file.length(), &self.spec)
            .map_err(UpdateError::Header)
    }

    fn validate_target(&self, header: package::UpdateHeader) -> Result<(), UpdateError> {
        if header.target_hw_id != self.spec.target_hw_id
            || header.target_fpga_id != self.spec.target_fpga_id
        {
            return Err(UpdateError::WrongTarget);
        }
        Ok(())
    }

    fn validate_layout(&self, header: package::UpdateHeader) -> Result<(), UpdateError> {
        if header.target_flash_layout != self.spec.flash_layout_id {
            return Err(UpdateError::WrongLayout);
        }
        Ok(())
    }

    fn validate_payload_digest(
        &mut self,
        card: &mut UpdaterSdCard<Io>,
        header: package::UpdateHeader,
    ) -> Result<(), UpdateError> {
        let volume = card.open_volume(VolumeIdx(0)).map_err(map_sd_error)?;
        let root = volume.open_root_dir().map_err(map_sd_error)?;
        let file = root
            .open_file_in_dir(self.spec.file_name, Mode::ReadOnly)
            .map_err(map_sd_error)?;
        file.seek_from_start(header.payload_offset)
            .map_err(map_sd_error)?;

        let mut crc = crc32::Crc32::new();
        let mut remaining = header.payload_size;

        while remaining != 0 {
            let chunk_len = core::cmp::min(remaining, self.page.len() as u32) as usize;
            read_exact(&file, &mut self.page[..chunk_len])?;
            crc.update(&self.page[..chunk_len]);
            remaining -= chunk_len as u32;
        }

        if crc.finish() != header.payload_crc32 {
            return Err(UpdateError::PayloadCrc);
        }
        Ok(())
    }

    fn program_app_slot(&mut self, header: package::UpdateHeader) -> Result<(), UpdateError> {
        if !self
            .spec
            .is_app_range(self.spec.app_base, header.payload_size)
        {
            return Err(UpdateError::SlotRange);
        }

        self.verify_flash_device()?;
        self.erase_app_slot(header.payload_size)?;
        self.program_payload_from_sd(header)?;
        self.io.sd_set_cs(false);
        Ok(())
    }

    fn verify_flash_device(&mut self) -> Result<(), UpdateError> {
        self.report_status(UpdateStatus::FlashJedec);
        let jedec = self.io.flash_jedec_id().map_err(UpdateError::Flash)?;
        if spi_nor::is_supported_jedec_id(jedec) {
            Ok(())
        } else {
            Err(UpdateError::FlashJedec)
        }
    }

    fn erase_app_slot(&mut self, payload_size: u32) -> Result<(), UpdateError> {
        const ERASE_SIZE: u32 = spi_nor::BLOCK_SIZE;

        let erase_end = self
            .spec
            .app_base
            .checked_add(payload_size)
            .and_then(|end| end.checked_add(ERASE_SIZE - 1))
            .map(|end| end & !(ERASE_SIZE - 1))
            .ok_or(UpdateError::SlotRange)?;

        if erase_end > self.spec.app_base + self.spec.app_size {
            return Err(UpdateError::SlotRange);
        }

        let mut address = self.spec.app_base;
        while address < erase_end {
            self.io
                .flash_erase_64k(address)
                .map_err(UpdateError::Flash)?;
            address += ERASE_SIZE;
        }
        Ok(())
    }

    fn program_payload_from_sd(
        &mut self,
        header: package::UpdateHeader,
    ) -> Result<(), UpdateError> {
        let card = self.open_sd_card()?;
        let volume = card.open_volume(VolumeIdx(0)).map_err(map_sd_error)?;
        let root = volume.open_root_dir().map_err(map_sd_error)?;
        let file = root
            .open_file_in_dir(self.spec.file_name, Mode::ReadOnly)
            .map_err(map_sd_error)?;
        file.seek_from_start(header.payload_offset)
            .map_err(map_sd_error)?;

        self.report_status(UpdateStatus::FlashProgram);
        let mut remaining = header.payload_size;
        let mut flash_address = self.spec.app_base;
        while remaining != 0 {
            let chunk_len = core::cmp::min(remaining, self.page.len() as u32) as usize;
            read_exact(&file, &mut self.page[..chunk_len])?;
            self.program_and_verify_page(flash_address, chunk_len)?;

            remaining -= chunk_len as u32;
            flash_address += chunk_len as u32;
        }
        Ok(())
    }

    fn program_and_verify_page(
        &mut self,
        flash_address: u32,
        chunk_len: usize,
    ) -> Result<(), UpdateError> {
        self.io
            .flash_program_page(flash_address, &self.page[..chunk_len])
            .map_err(UpdateError::Flash)?;

        self.report_status(UpdateStatus::FlashVerify);
        self.io
            .flash_read(flash_address, &mut self.verify[..chunk_len])
            .map_err(UpdateError::Flash)?;
        if self.verify[..chunk_len] != self.page[..chunk_len] {
            return Err(UpdateError::VerifyMismatch);
        }
        self.report_status(UpdateStatus::FlashProgram);
        Ok(())
    }

    fn trigger_app_reconfig(&mut self) {
        self.io.set_reconfig_trigger(true);
        let mut delay = FirmwareDelay;
        delay.delay_ms(RECONFIG_PULSE_MS);
        self.io.set_reconfig_trigger(false);
    }
}

type UpdaterSdCard<Io> =
    VolumeManager<SdCard<MmioSdSpi<'static, Io>, FirmwareDelay>, NullTime, 4, 4, 1>;

#[derive(Clone, Copy)]
struct NullTime;

impl TimeSource for NullTime {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 56,
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
        }
    }
}

fn read_exact<D, T, const DIRS: usize, const FILES: usize, const VOLUMES: usize>(
    file: &embedded_sdmmc::File<'_, D, T, DIRS, FILES, VOLUMES>,
    mut out: &mut [u8],
) -> Result<(), UpdateError>
where
    D: embedded_sdmmc::BlockDevice,
    D::Error: core::fmt::Debug,
    T: TimeSource,
{
    while !out.is_empty() {
        let read = file.read(out).map_err(map_sd_error)?;
        if read == 0 {
            return Err(UpdateError::SdFormat);
        }
        let tmp = out;
        out = &mut tmp[read..];
    }
    Ok(())
}

fn map_sd_error<E: core::fmt::Debug>(error: embedded_sdmmc::Error<E>) -> UpdateError {
    match error {
        embedded_sdmmc::Error::DeviceError(_) => UpdateError::SdIo,
        embedded_sdmmc::Error::NotFound => UpdateError::SdFileNotFound,
        _ => UpdateError::SdFormat,
    }
}

fn map_update_error(error: UpdateError) -> UpdateStatus {
    match error {
        UpdateError::SdIo => UpdateStatus::FatIoError,
        UpdateError::SdFileNotFound => UpdateStatus::FileNotFound,
        UpdateError::SdFormat => UpdateStatus::FatFormatError,
        UpdateError::Header(_) => UpdateStatus::HeaderError,
        UpdateError::WrongTarget | UpdateError::WrongLayout => UpdateStatus::TargetError,
        UpdateError::PayloadCrc | UpdateError::SlotRange => UpdateStatus::PayloadError,
        UpdateError::Flash(_) | UpdateError::FlashJedec | UpdateError::VerifyMismatch => {
            UpdateStatus::FlashError
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::TEST_SPEC;
    use crate::test_doubles::FakeMmio;

    fn header_for(payload: &[u8]) -> package::UpdateHeader {
        package::UpdateHeader {
            format_version: 0,
            target_hw_id: 0,
            target_fpga_id: 0,
            target_flash_layout: 0,
            app_version: 0,
            payload_offset: 0,
            payload_size: payload.len() as u32,
            payload_crc32: crc32::checksum(payload),
        }
    }

    #[test]
    fn app_slot_matches_when_content_identical() {
        let payload = vec![0x5A; 1024];
        let mut mmio = FakeMmio::default();
        let start = TEST_SPEC.app_base as usize;
        mmio.flash[start..start + payload.len()].copy_from_slice(&payload);
        let mut updater = Updater::new(mmio, TEST_SPEC);
        assert!(updater.app_slot_matches(header_for(&payload)).unwrap());
    }

    #[test]
    fn app_slot_differs_when_content_corrupted() {
        let payload = vec![0x5A; 1024];
        let mut corrupted = payload.clone();
        corrupted[500] ^= 0xFF;
        let mut mmio = FakeMmio::default();
        let start = TEST_SPEC.app_base as usize;
        mmio.flash[start..start + corrupted.len()].copy_from_slice(&corrupted);
        let mut updater = Updater::new(mmio, TEST_SPEC);
        assert!(!updater.app_slot_matches(header_for(&payload)).unwrap());
    }

    #[test]
    fn app_slot_differs_when_slot_empty() {
        let payload = vec![0x5A; 1024];
        let mmio = FakeMmio::default();
        let mut updater = Updater::new(mmio, TEST_SPEC);
        assert!(!updater.app_slot_matches(header_for(&payload)).unwrap());
    }

    #[test]
    fn app_slot_matches_reports_flash_error() {
        let payload = vec![0x5A; 1024];
        let mmio = FakeMmio {
            fail_io: true,
            ..Default::default()
        };
        let mut updater = Updater::new(mmio, TEST_SPEC);
        assert!(updater.app_slot_matches(header_for(&payload)).is_err());
    }
}
