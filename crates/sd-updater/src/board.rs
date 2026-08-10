//! `Updater` が要求するボード抽象。プロジェクト側はこれを実装して MMIO に接続する。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoError {
    OutOfRange,
    Hardware,
    Timeout,
}

impl embedded_hal::spi::Error for IoError {
    fn kind(&self) -> embedded_hal::spi::ErrorKind {
        embedded_hal::spi::ErrorKind::Other
    }
}

/// SD / Flash / reconfigを抽象化するポート。
///
/// 実装例は各プロジェクトの `mmio.rs` を参照。SD と Flash は同時に使わない契約で、
/// 呼び出し順の直列化は updater 側が保証する。
pub trait BoardIo {
    fn status(&self) -> u32;
    /// 4bit の状態表示コードを RTL の state 出力へ書き込む (0x0-0xF)。
    fn set_state(&mut self, state: u32);
    fn sd_set_cs(&mut self, asserted: bool);
    fn sd_set_clock_div(&mut self, half_period_cycles: u8);
    fn sd_transfer_byte(&mut self, byte: u8) -> Result<u8, IoError>;
    /// SD カードが挿入されているか。未挿入なら SD 初期化のリトライ待ちをせずに
    /// app へ移行する経路で使う。極性は実装 (mmio) 側で解釈する。
    fn sd_card_detect(&self) -> bool;
    fn flash_erase_64k(&mut self, address: u32) -> Result<(), IoError>;
    fn flash_program_page(&mut self, address: u32, data: &[u8]) -> Result<(), IoError>;
    fn flash_read(&mut self, address: u32, out: &mut [u8]) -> Result<(), IoError>;
    fn flash_jedec_id(&mut self) -> Result<[u8; 3], IoError>;
    fn set_reconfig_trigger(&mut self, asserted_low: bool);
}
