//! プロジェクト固有の update format / flash layout 定義。
//!
//! `sd-updater-build` が `update_spec.conf` から生成する定数を組み立て、
//! [`crate::Updater`] に渡す。値の基準は常にプロジェクト側の `update_spec.conf` であり、
//! この構造体に値を直書きしないこと。

/// header バッファの上限。
///
/// `header_size` はランタイム値のため配列長に使えない。`read_header` はこのサイズの
/// バッファで header を読み、`header_size` がこれを超える場合は parse が失敗する。
pub const MAX_HEADER_SIZE: usize = 0x100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateSpec {
    pub file_name: &'static str,
    pub magic: [u8; 8],
    pub header_size: usize,
    pub format_version: u32,
    pub target_hw_id: u32,
    pub target_fpga_id: u32,
    pub flash_layout_id: u32,
    pub flash_size_bytes: u32,
    pub updater_base: u32,
    pub updater_size: u32,
    pub app_base: u32,
    pub app_size: u32,
    pub metadata_base: u32,
    pub metadata_size: u32,
    pub golden_updater_base_candidate: u32,
    pub golden_updater_size_candidate: u32,
}

impl UpdateSpec {
    /// `address` からの `length` バイトが app slot に完全に収まるか。
    pub const fn is_app_range(&self, address: u32, length: u32) -> bool {
        if length == 0 {
            return false;
        }

        let Some(end) = address.checked_add(length - 1) else {
            return false;
        };

        address >= self.app_base && end < self.app_base + self.app_size
    }

    /// payload が app slot に入るサイズか。
    pub const fn payload_fits_app_slot(&self, length: u32) -> bool {
        length != 0 && length <= self.app_size
    }

    /// metadata 領域の直後から始まる将来拡張用プールの先頭。
    pub const fn expansion_pool_base(&self) -> u32 {
        self.metadata_base + self.metadata_size
    }

    /// 拡張プールのサイズ。
    pub const fn expansion_pool_size(&self) -> u32 {
        self.golden_updater_base_candidate - self.expansion_pool_base()
    }
}

#[cfg(test)]
pub(crate) const TEST_SPEC: UpdateSpec = UpdateSpec {
    file_name: "FPGAOSC.UPD",
    magic: [0x46, 0x47, 0x41, 0x4F, 0x53, 0x43, 0x00, 0x00],
    header_size: 0x58,
    format_version: 1,
    target_hw_id: 0x4650_4F53,
    target_fpga_id: 0x4757_3525,
    flash_layout_id: 0x4650_4F31,
    flash_size_bytes: 0x800_000,
    updater_base: 0x000_000,
    updater_size: 0x100_000,
    app_base: 0x100_000,
    app_size: 0x100_000,
    metadata_base: 0x200_000,
    metadata_size: 0x010_000,
    golden_updater_base_candidate: 0x700_000,
    golden_updater_size_candidate: 0x100_000,
};

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = assert!(TEST_SPEC.header_size <= MAX_HEADER_SIZE);

    #[test]
    fn app_range_guard_allows_only_app_slot() {
        let spec = TEST_SPEC;
        assert!(spec.is_app_range(spec.app_base, 1));
        assert!(spec.is_app_range(spec.app_base, spec.app_size));
        assert!(!spec.is_app_range(spec.app_base - 1, 1));
        assert!(!spec.is_app_range(spec.app_base + spec.app_size, 1));
        assert!(!spec.is_app_range(spec.app_base, spec.app_size + 1));
        assert!(!spec.is_app_range(spec.app_base, 0));
    }
}
