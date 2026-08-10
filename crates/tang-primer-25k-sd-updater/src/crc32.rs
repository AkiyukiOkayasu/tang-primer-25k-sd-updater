const POLY: u32 = 0xEDB8_8320;

/// バイト単位の CRC32 用テーブル (1KB)。PicoRV32 のようなシフトが遅い CPU 向けに
/// ビット単位処理をテーブル駆動に置き換えた (約 8 倍高速)。
const TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = index as u32;
        let mut bit = 0;
        while bit < 8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (POLY & mask);
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
};

#[derive(Debug, Clone, Copy)]
pub struct Crc32 {
    state: u32,
}

impl Crc32 {
    pub const fn new() -> Self {
        Self { state: 0xFFFF_FFFF }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            let index = ((self.state ^ *byte as u32) & 0xFF) as usize;
            self.state = TABLE[index] ^ (self.state >> 8);
        }
    }

    pub const fn finish(self) -> u32 {
        !self.state
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

pub fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(bytes);
    crc.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_standard_test_vector() {
        assert_eq!(checksum(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn matches_bit_at_a_time_reference() {
        // テーブル駆動がビット単位処理と同結果であることを確認
        fn bitwise(bytes: &[u8]) -> u32 {
            let mut state = 0xFFFF_FFFFu32;
            for byte in bytes {
                let mut crc = state ^ (*byte as u32);
                for _ in 0..8 {
                    let mask = 0u32.wrapping_sub(crc & 1);
                    crc = (crc >> 1) ^ (POLY & mask);
                }
                state = crc;
            }
            !state
        }
        assert_eq!(checksum(b""), bitwise(b""));
        assert_eq!(checksum(b"123456789"), bitwise(b"123456789"));
        let data: Vec<u8> = (0..=255u8).collect();
        assert_eq!(checksum(&data), bitwise(&data));
    }
}
