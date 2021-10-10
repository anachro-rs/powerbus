pub const TTL_PAGES: usize = 256;
pub const PAGE_SIZE: usize = 4096;

pub const SECTION_0_START: usize = 0;

pub const SECTION_1_START_METADATA: usize = PAGE_SIZE * 64;
pub const SECTION_1_START_APP: usize = PAGE_SIZE * (64 + 1);

pub const SECTION_2_START_METADATA: usize = PAGE_SIZE * 128;
pub const SECTION_2_START_APP: usize = PAGE_SIZE * (128 + 1);

pub const SECTION_3_START_METADATA: usize = PAGE_SIZE * 192;
pub const SECTION_3_START_APP: usize = PAGE_SIZE * (192 + 1);

pub const POLY_1305_KEY: &[u8; 32] = b"Anachro: a thing out of its time";
pub const POLY_TAG_SIZE: usize = 16;
