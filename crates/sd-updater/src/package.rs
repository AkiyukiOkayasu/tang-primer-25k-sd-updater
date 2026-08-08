//! update package の header 解釈。

use crate::UpdateSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderError {
    TooShort,
    BadMagic,
    UnsupportedFormat,
    WrongFlashLayout,
    BadPayloadOffset,
    BadPayloadSize,
    PayloadOutOfFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateHeader {
    pub format_version: u32,
    pub target_hw_id: u32,
    pub target_fpga_id: u32,
    pub target_flash_layout: u32,
    pub app_version: u32,
    pub payload_offset: u32,
    pub payload_size: u32,
    pub payload_crc32: u32,
    pub payload_sha256: [u8; 32],
}

impl UpdateHeader {
    /// `bytes` は `[0u8; MAX_HEADER_SIZE]` バッファの全体でもよい。
    /// `header_size` を超える size 指定は [`HeaderError::TooShort`] になる。
    pub fn parse(bytes: &[u8], file_size: u32, spec: &UpdateSpec) -> Result<Self, HeaderError> {
        if bytes.len() < spec.header_size {
            return Err(HeaderError::TooShort);
        }
        if bytes[0..8] != spec.magic {
            return Err(HeaderError::BadMagic);
        }

        let header = Self {
            format_version: read_u32_le(bytes, 0x08),
            target_hw_id: read_u32_le(bytes, 0x0C),
            target_fpga_id: read_u32_le(bytes, 0x10),
            target_flash_layout: read_u32_le(bytes, 0x14),
            app_version: read_u32_le(bytes, 0x18),
            payload_offset: read_u32_le(bytes, 0x1C),
            payload_size: read_u32_le(bytes, 0x20),
            payload_crc32: read_u32_le(bytes, 0x24),
            payload_sha256: read_array_32(bytes, 0x28),
        };

        header.validate(file_size, spec)?;
        Ok(header)
    }

    pub fn validate(&self, file_size: u32, spec: &UpdateSpec) -> Result<(), HeaderError> {
        if self.format_version != spec.format_version {
            return Err(HeaderError::UnsupportedFormat);
        }
        if self.target_flash_layout != spec.flash_layout_id {
            return Err(HeaderError::WrongFlashLayout);
        }
        if self.payload_offset < spec.header_size as u32 {
            return Err(HeaderError::BadPayloadOffset);
        }
        if !spec.payload_fits_app_slot(self.payload_size) {
            return Err(HeaderError::BadPayloadSize);
        }

        let Some(payload_end) = self.payload_offset.checked_add(self.payload_size) else {
            return Err(HeaderError::PayloadOutOfFile);
        };
        if payload_end > file_size {
            return Err(HeaderError::PayloadOutOfFile);
        }

        Ok(())
    }
}

const fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    (bytes[offset] as u32)
        | ((bytes[offset + 1] as u32) << 8)
        | ((bytes[offset + 2] as u32) << 16)
        | ((bytes[offset + 3] as u32) << 24)
}

fn read_array_32(bytes: &[u8], offset: usize) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes[offset..offset + 32]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::MAX_HEADER_SIZE;
    use crate::spec::TEST_SPEC;

    #[test]
    fn parses_valid_header() {
        let mut bytes = [0u8; MAX_HEADER_SIZE];
        bytes[0..8].copy_from_slice(&TEST_SPEC.magic);
        write_u32_le(&mut bytes, 0x08, TEST_SPEC.format_version);
        write_u32_le(&mut bytes, 0x14, TEST_SPEC.flash_layout_id);
        write_u32_le(&mut bytes, 0x1C, TEST_SPEC.header_size as u32);
        write_u32_le(&mut bytes, 0x20, 1024);

        let header =
            UpdateHeader::parse(&bytes, TEST_SPEC.header_size as u32 + 1024, &TEST_SPEC).unwrap();
        assert_eq!(header.payload_offset, TEST_SPEC.header_size as u32);
        assert_eq!(header.payload_size, 1024);
    }

    #[test]
    fn rejects_payload_larger_than_app_slot() {
        let mut bytes = [0u8; MAX_HEADER_SIZE];
        bytes[0..8].copy_from_slice(&TEST_SPEC.magic);
        write_u32_le(&mut bytes, 0x08, TEST_SPEC.format_version);
        write_u32_le(&mut bytes, 0x14, TEST_SPEC.flash_layout_id);
        write_u32_le(&mut bytes, 0x1C, TEST_SPEC.header_size as u32);
        write_u32_le(&mut bytes, 0x20, TEST_SPEC.app_size + 1);

        assert_eq!(
            UpdateHeader::parse(
                &bytes,
                TEST_SPEC.header_size as u32 + TEST_SPEC.app_size + 1,
                &TEST_SPEC
            ),
            Err(HeaderError::BadPayloadSize)
        );
    }

    fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset] = value as u8;
        bytes[offset + 1] = (value >> 8) as u8;
        bytes[offset + 2] = (value >> 16) as u8;
        bytes[offset + 3] = (value >> 24) as u8;
    }
}
