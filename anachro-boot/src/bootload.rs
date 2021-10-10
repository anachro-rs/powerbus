//! HAL interface to the Non-Volatile Memory Controller (NVMC) peripheral.
//! Modified from the upstream nrf-hal-common crate

use core::ptr::NonNull;

use nrf52840_hal::nvmc::Instance;
use poly1305::{Block, Key, Poly1305, universal_hash::{NewUniversalHash, UniversalHash}};

use crate::consts::PAGE_SIZE;

pub const BOOTY_CHUNK_SIZE: usize = 256;

pub struct Booty<T: Instance> {
    nvmc: T,
    active: Option<FlashingSection>,
}

pub struct Metadata {
    pub image_uuid: [u8; 16],
    pub image_poly1305_tag: [u8; 16],
    pub image_len_pages: usize,
    pub boot_seq_number: u32,
    pub flashed_tagword: u32,
    pub booted_tagword: u32,
    pub app_ptr: NonNull<u8>,
}

impl Metadata {
    fn invalid_from_app_ptr(putter: NonNull<u8>) -> Self {
        Self {
            image_uuid: [0u8; 16],
            image_poly1305_tag: [0u8; 16],
            image_len_pages: 0,
            boot_seq_number: 0,
            flashed_tagword: 0,
            booted_tagword: 0,
            app_ptr: putter,
        }
    }

    pub fn generate_poly_tag(&self) -> Option<[u8; 16]> {
        let key = Key::from_slice(crate::consts::POLY_1305_KEY);
        let mut poly = Poly1305::new(key);

        match self.image_len_pages {
            0 => {
                defmt::error!("Zero pages!");
                return None
            }
            1..=255 => {},
            _ => {
                defmt::error!("Too many pages!");
                return None
            }
        }

        let uuid_all_zeroes = self.image_uuid.iter().all(|b| *b == 0);
        let uuid_all_effffs = self.image_uuid.iter().all(|b| *b == 0xFF);

        if uuid_all_zeroes || uuid_all_effffs {
            defmt::error!("Bad UUID!");
            return None;
        }

        for page in 0..self.image_len_pages {
            let offset = page * crate::consts::PAGE_SIZE;
            let page_slice = unsafe {
                core::slice::from_raw_parts(
                    self.app_ptr.as_ptr().add(offset),
                    crate::consts::PAGE_SIZE,
                )
            };

            for chunk in page_slice.chunks_exact(crate::consts::POLY_TAG_SIZE) {
                poly.update(Block::from_slice(chunk));
            }
        }

        let result = poly.finalize().into_bytes().into();

        defmt::trace!(
            "App at {=u32} (poly)=> {:?}",
            self.app_ptr.as_ptr() as u32,
            &result,
        );

        Some(result)
    }

    pub fn from_section(section: UsableSections) -> Option<Metadata> {
        let base_app_nn = NonNull::new(section.app_as_ptr())?;
        let mut meta = Metadata::invalid_from_app_ptr(base_app_nn);

        let base: *mut u8 = section.metadata_as_ptr();

        cortex_m::asm::dmb();

        // NOTE: We know that all elements are correctly aligned and not overlapping
        unsafe {
            meta.image_uuid = core::ptr::read(base.add(0).cast());
            meta.image_poly1305_tag = core::ptr::read(base.add(16).cast());
            meta.image_len_pages = core::ptr::read(base.add(32).cast());

            meta.boot_seq_number = core::ptr::read(base.add(128).cast());
            meta.flashed_tagword = core::ptr::read(base.add(132).cast());
            meta.booted_tagword  = core::ptr::read(base.add(136).cast());
        }

        // TODO: Validate!

        Some(meta)
    }
}

struct FlashingSection {
    section: UsableSections,
    cur_page: usize,
    cur_page_offset_bytes: usize,
    cur_page_erased: bool,
}

impl<T: Instance> Booty<T> {
    pub fn new(nvmc: T) -> Self {
        Self {
            nvmc,
            active: None,
        }
    }
}

pub enum UsableSections {
    Section1,
    Section2,
    Section3,
}

impl UsableSections {
    fn metadata_as_ptr(&self) -> *mut u8 {
        let usize_addr = match self {
            UsableSections::Section1 => crate::consts::SECTION_1_START_METADATA,
            UsableSections::Section2 => crate::consts::SECTION_2_START_METADATA,
            UsableSections::Section3 => crate::consts::SECTION_3_START_METADATA,
        };

        usize_addr as *const u8 as *mut u8
    }

    fn app_as_ptr(&self) -> *mut u8 {
        let usize_addr = match self {
            UsableSections::Section1 => crate::consts::SECTION_1_START_APP,
            UsableSections::Section2 => crate::consts::SECTION_2_START_APP,
            UsableSections::Section3 => crate::consts::SECTION_3_START_APP,
        };

        usize_addr as *const u8 as *mut u8
    }
}


// NOTE: Datasheet says 85ms, take it up to 100ms for some healthy
// fudge factor
const TOTAL_PAGE_ERASE_MS: u32 = 100;
struct PartialErase {
    total_ms: u32,
    step_ms: u32,
    page: u32,
}

pub enum PartialStatus {
    Done,
    RemainingMs(u32),
}

/// Interface to an NVMC instance.
pub struct Nvmc<'a, T: Instance> {
    nvmc: &'a T,
    section: UsableSections,

    // TODO: Refuse to do certain things when this is some
    wip_erase_ms: Option<PartialErase>,
}

impl<'a, T> Nvmc<'a, T>
where
    T: Instance,
{
    /// Takes ownership of the peripheral and storage area.
    pub fn new(nvmc: &'a T, section: UsableSections) -> Nvmc<'a, T> {
        Self { nvmc, section, wip_erase_ms: None }
    }

    pub fn start_partial_erase(&mut self, mut steps_ms: u32, start_addr: usize) -> Result<PartialStatus, ()> {
        if self.wip_erase_ms.is_some() {
            return Err(());
        }

        // TODO: also check valid range
        if start_addr & (PAGE_SIZE - 1) != 0 {
            return Err(());
        }

        if steps_ms >= TOTAL_PAGE_ERASE_MS {
            steps_ms = TOTAL_PAGE_ERASE_MS;
        }

        self.nvmc.erasepagepartialcfg.write(|w| unsafe {
            w.duration().bits(steps_ms as u8)
        });

        self.wip_erase_ms = Some(PartialErase {
            total_ms: 0,
            step_ms: steps_ms,
            page: start_addr as u32,
        });

        Ok(PartialStatus::RemainingMs(TOTAL_PAGE_ERASE_MS))
    }

    // NOTE: DOES set write enable!
    pub fn step_partial_erase(&mut self) -> Result<PartialStatus, ()> {
        let mut wip = self.wip_erase_ms.take().ok_or(())?;

        self.enable_erase();
        self.nvmc.erasepagepartial.write(|w| unsafe {
            w.erasepagepartial().bits(wip.page)
        });
        self.wait_ready();
        self.enable_read();

        wip.total_ms += wip.step_ms;

        if wip.total_ms >= TOTAL_PAGE_ERASE_MS {
            Ok(PartialStatus::Done)
        } else {
            let remain = TOTAL_PAGE_ERASE_MS - wip.total_ms;
            self.wip_erase_ms = Some(wip);
            Ok(PartialStatus::RemainingMs(remain))
        }
    }

    pub fn enable_erase(&self) {
        self.nvmc.config.write(|w| w.wen().een());
    }

    pub fn enable_read(&self) {
        self.nvmc.config.write(|w| w.wen().ren());
    }

    pub fn enable_write(&self) {
        self.nvmc.config.write(|w| w.wen().wen());
    }

    #[inline]
    pub fn wait_ready(&self) {
        while !self.nvmc.ready.read().ready().bit_is_set() {}
    }

    #[inline]
    pub fn erase_metadata(&mut self) {
        let bits = self.section.metadata_as_ptr() as u32;
        self.nvmc.erasepage().write(|w| unsafe { w.bits(bits) });
        self.wait_ready();
    }

    #[inline]
    pub fn erase_app_page(&mut self, page: usize) {
        defmt::assert!(page < 63);

        let bits = self.section.app_as_ptr() as usize;
        let offset = crate::consts::PAGE_SIZE * page;
        let bits = (bits + offset) as u32;

        self.nvmc.erasepage().write(|w| unsafe { w.bits(bits) });
        self.wait_ready();
    }

    #[inline]
    pub fn write_word_app(&mut self, offset_bytes: usize, word: u32) {
        defmt::info!("{:?}", offset_bytes);
        defmt::assert_eq!(offset_bytes & 0b11, 0);

        self.wait_ready();
        let base = self.section.app_as_ptr() as usize;
        let bits = base + offset_bytes;

        let mut_ptr = bits as *const u32 as *mut u32;

        unsafe {
            mut_ptr.write_volatile(word);
        }

        cortex_m::asm::dmb();
    }

    #[inline]
    pub fn write_word_meta(&mut self, offset_bytes: usize, word: u32) {
        defmt::assert_eq!(offset_bytes & 0b11, 0);

        self.wait_ready();
        let base = self.section.metadata_as_ptr() as usize;
        let bits = base + offset_bytes;

        let mut_ptr = bits as *const u32 as *mut u32;

        unsafe {
            mut_ptr.write_volatile(word);
        }

        cortex_m::asm::dmb();
    }
}

// impl<T> ReadNorFlash for Nvmc<T>
// where
//     T: Instance,
// {
//     type Error = NvmcError;

//     const READ_SIZE: usize = 4;

//     fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
//         let offset = offset as usize;
//         let bytes_len = bytes.len();
//         let read_len = bytes_len + (Self::READ_SIZE - (bytes_len % Self::READ_SIZE));
//         let target_offset = offset + read_len;
//         if offset % Self::READ_SIZE == 0 && target_offset <= self.capacity() {
//             self.wait_ready();
//             let last_offset = target_offset - Self::READ_SIZE;
//             for offset in (offset..last_offset).step_by(Self::READ_SIZE) {
//                 let word = self.storage[offset >> 2];
//                 bytes[offset] = (word >> 24) as u8;
//                 bytes[offset + 1] = (word >> 16) as u8;
//                 bytes[offset + 2] = (word >> 8) as u8;
//                 bytes[offset + 3] = (word >> 0) as u8;
//             }
//             let offset = last_offset;
//             let word = self.storage[offset >> 2];
//             let mut bytes_offset = offset;
//             if bytes_offset < bytes_len {
//                 bytes[bytes_offset] = (word >> 24) as u8;
//                 bytes_offset += 1;
//                 if bytes_offset < bytes_len {
//                     bytes[bytes_offset] = (word >> 16) as u8;
//                     bytes_offset += 1;
//                     if bytes_offset < bytes_len {
//                         bytes[bytes_offset] = (word >> 8) as u8;
//                         bytes_offset += 1;
//                         if bytes_offset < bytes_len {
//                             bytes[bytes_offset] = (word >> 0) as u8;
//                         }
//                     }
//                 }
//             }
//             Ok(())
//         } else {
//             Err(NvmcError::Unaligned)
//         }
//     }

//     fn capacity(&self) -> usize {
//         self.storage.len() << 2
//     }
// }

// impl<T> NorFlash for Nvmc<T>
// where
//     T: Instance,
// {
//     const WRITE_SIZE: usize = 4;

//     const ERASE_SIZE: usize = 4 * 1024;

//     fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
//         if from as usize % Self::ERASE_SIZE == 0 && to as usize % Self::ERASE_SIZE == 0 {
//             self.enable_erase();
//             for offset in (from..to).step_by(Self::ERASE_SIZE) {
//                 self.erase_page(offset as usize >> 2);
//             }
//             self.enable_read();
//             Ok(())
//         } else {
//             Err(NvmcError::Unaligned)
//         }
//     }

//     fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
//         let offset = offset as usize;
//         if offset % Self::WRITE_SIZE == 0 && bytes.len() % Self::WRITE_SIZE == 0 {
//             self.enable_write();
//             for offset in (offset..(offset + bytes.len())).step_by(Self::WRITE_SIZE) {
//                 let word = ((bytes[offset] as u32) << 24)
//                     | ((bytes[offset + 1] as u32) << 16)
//                     | ((bytes[offset + 2] as u32) << 8)
//                     | ((bytes[offset + 3] as u32) << 0);
//                 self.write_word(offset >> 2, word);
//             }
//             self.enable_read();
//             Ok(())
//         } else {
//             Err(NvmcError::Unaligned)
//         }
//     }
// }
