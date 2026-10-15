//! CRC32 and CRC32C.
//!
//! Both are reflected table-driven polynomial CRCs with the raw convention
//! used by the kernel drivers: the caller supplies the initial register value
//! (usually `!0`) and the result is returned without a final inversion. GPT
//! uses the IEEE polynomial; ext4 metadata checksums use CRC32C.

const POLY_CRC32: u32 = 0xEDB8_8320;
const POLY_CRC32C: u32 = 0x82F6_3B78;

const fn build_table(poly: u32) -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ poly
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

static TABLE_CRC32: [u32; 256] = build_table(POLY_CRC32);
static TABLE_CRC32C: [u32; 256] = build_table(POLY_CRC32C);

#[inline]
fn update(table: &[u32; 256], mut crc: u32, data: &[u8]) -> u32 {
    for &byte in data {
        crc = (crc >> 8) ^ table[((crc ^ byte as u32) & 0xFF) as usize];
    }
    crc
}

pub fn crc32(crc: u32, data: &[u8]) -> u32 {
    update(&TABLE_CRC32, crc, data)
}

pub fn crc32c(crc: u32, data: &[u8]) -> u32 {
    update(&TABLE_CRC32C, crc, data)
}