//! 64Mbit クラス SPI NOR Flash の共通ジオメトリとコマンド。
//!
//! JEDEC ID は容量 byte (0x17 = 64Mbit) のみで判定し、manufacturer/type は固定しない。
//! 標準 SPI NOR コマンドセット (0x9F/0x05/0x06/0x20/0xD8/0x02/0x03/0x0B) を使う。

/// 64Mbit (8MiB) SPI NOR の容量。
pub const CAPACITY_BYTES: u32 = 8 * 1024 * 1024;
pub const PAGE_SIZE: u32 = 256;
pub const SECTOR_SIZE: u32 = 4 * 1024;
pub const BLOCK_SIZE: u32 = 64 * 1024;
pub const ADDRESS_BYTES: usize = 3;

/// 受け入れ判定のテストに使う 64Mbit SPI NOR の JEDEC ID 例。
pub const EXAMPLE_JEDEC_ID: [u8; 3] = [0xEF, 0x40, 0x17];

/// 64Mbit SPI NOR として扱える JEDEC ID だけを受け入れる。
/// manufacturer/type は固定せず、容量 byte (0x17 = 64Mbit) と
/// 明らかな未接続値 (all-zero / all-one) だけを見る。
/// 他の manufacturer の 64Mbit SPI NOR でも同じコマンドセットで扱える。
pub const fn is_supported_jedec_id(id: [u8; 3]) -> bool {
    let all_zero = id[0] == 0x00 && id[1] == 0x00 && id[2] == 0x00;
    let all_one = id[0] == 0xFF && id[1] == 0xFF && id[2] == 0xFF;
    !all_zero && !all_one && id[2] == 0x17
}

pub mod command {
    pub const READ_JEDEC_ID: u8 = 0x9F;
    pub const READ_STATUS1: u8 = 0x05;
    pub const WRITE_ENABLE: u8 = 0x06;
    pub const SECTOR_ERASE_4K: u8 = 0x20;
    pub const BLOCK_ERASE_64K: u8 = 0xD8;
    pub const PAGE_PROGRAM: u8 = 0x02;
    pub const READ_DATA: u8 = 0x03;
    pub const FAST_READ: u8 = 0x0B;
}

pub mod status1 {
    pub const BUSY: u8 = 1 << 0;
    pub const WRITE_ENABLE_LATCH: u8 = 1 << 1;
}

pub const fn is_valid_address(address: u32) -> bool {
    address < CAPACITY_BYTES
}

pub const fn is_page_aligned(address: u32) -> bool {
    address.is_multiple_of(PAGE_SIZE)
}

pub const fn is_sector_aligned(address: u32) -> bool {
    address.is_multiple_of(SECTOR_SIZE)
}

pub const fn is_block_aligned(address: u32) -> bool {
    address.is_multiple_of(BLOCK_SIZE)
}

pub const fn page_program_len_ok(address: u32, length: u32) -> bool {
    if length == 0 || length > PAGE_SIZE {
        return false;
    }

    let page_offset = address % PAGE_SIZE;
    page_offset + length <= PAGE_SIZE
}

pub fn address_bytes(address: u32) -> [u8; ADDRESS_BYTES] {
    [
        ((address >> 16) & 0xFF) as u8,
        ((address >> 8) & 0xFF) as u8,
        (address & 0xFF) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_three_byte_address() {
        assert_eq!(address_bytes(0x12_3456), [0x12, 0x34, 0x56]);
    }

    #[test]
    fn page_program_does_not_cross_page_boundary() {
        assert!(page_program_len_ok(0x000100, 256));
        assert!(page_program_len_ok(0x0001F0, 16));
        assert!(!page_program_len_ok(0x0001F0, 17));
        assert!(!page_program_len_ok(0x000100, 257));
    }

    #[test]
    fn accepts_64mbit_jedec_ids() {
        assert!(is_supported_jedec_id(EXAMPLE_JEDEC_ID));
        assert!(is_supported_jedec_id([0xC8, 0x40, 0x17]));
        assert!(!is_supported_jedec_id([0xFF, 0xFF, 0xFF]));
        assert!(!is_supported_jedec_id([0x00, 0x00, 0x00]));
        assert!(!is_supported_jedec_id([0xEF, 0x40, 0x16]));
    }
}
