//! HAL interface to the Non-Volatile Memory Controller (NVMC) peripheral.
//! Modified from the upstream nrf-hal-common crate

use core::ptr::NonNull;

use nrf52840_hal::{nvmc::Instance, pac::NVMC};
use poly1305::{Block, Key, Poly1305, universal_hash::{NewUniversalHash, UniversalHash}};

use crate::{bootdata::Bootdata, consts::{PAGE_SIZE, POLY_TAG_SIZE}};

#[derive(Debug, Clone)]
pub struct Metadata {
    pub section: UsableSections,

    pub image_uuid: [u8; 16],
    pub image_poly1305_tag: [u8; 16],
    pub image_len_pages: usize,
    pub boot_seq_number: u32,

    // NOTE: These two are only really relevant in the side a/b region
    pub flashed_tagword: u32,
    pub booted_tagword: u32,
    // END NOTE

    pub app_ptr: NonNull<u8>,
}



impl Metadata {
    pub const FLASHED_TAG_NOT_FLASHED: u32 = 0xFFFF_FFFF;
    pub const FLASHED_TAG_IS_FLASHED: u32 = 0xCAFE_FEED;

    pub const BOOTED_TAG_NOT_BOOTED: u32 = 0xFFFF_FFFF;
    pub const BOOTED_TAG_IS_BOOTED: u32 = 0xB007_B007;

    pub fn has_booted(&self) -> bool {
        self.booted_tagword == Self::BOOTED_TAG_IS_BOOTED
    }

    pub fn has_flashed(&self) -> bool {
        self.flashed_tagword == Self::FLASHED_TAG_IS_FLASHED
    }

    pub fn write_to_section(&self, nvmc: &NVMC, section: UsableSections, boot_seq: u32) {
        let base = section.metadata_as_ptr();
        let us = Nvmc::new(nvmc, section.clone());

        us.enable_write();

        unsafe {
            us.wait_ready();
            core::ptr::write_volatile(base.add(0).cast(), self.image_uuid);
            us.wait_ready();
            core::ptr::write_volatile(base.add(16).cast(), self.image_poly1305_tag);
            us.wait_ready();
            core::ptr::write_volatile(base.add(32).cast(), self.image_len_pages);

            us.wait_ready();
            core::ptr::write_volatile(base.add(128).cast(), boot_seq);

            us.wait_ready();
            core::ptr::write_volatile(base.add(132).cast(), Self::FLASHED_TAG_IS_FLASHED);

            // ALSO write the origin to mark it as flashed!
            if !self.has_flashed() {
                defmt::info!("Marking as flashed!");
                us.wait_ready();
                core::ptr::write_volatile(self.section.metadata_as_ptr().add(132).cast(), Self::FLASHED_TAG_IS_FLASHED);
            }

            us.wait_ready();
            core::ptr::write_volatile(base.add(136).cast(), Self::BOOTED_TAG_NOT_BOOTED);
        }

        cortex_m::asm::dmb();
    }

    pub fn from_section(section: UsableSections) -> Option<Metadata> {
        let base_app_nn = NonNull::new(section.app_as_ptr())?;
        let mut meta = Self {
            section: section.clone(),
            image_uuid: [0u8; 16],
            image_poly1305_tag: [0u8; 16],
            image_len_pages: 0,
            boot_seq_number: 0,
            flashed_tagword: 0,
            booted_tagword: 0,
            app_ptr: base_app_nn,
        };

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

        // Validate
        let flash_vals = [
            Self::FLASHED_TAG_NOT_FLASHED,
            Self::FLASHED_TAG_IS_FLASHED
        ];

        if !flash_vals.iter().any(|t| *t == meta.flashed_tagword) {
            defmt::error!("Bad flashed tag!");
            return None;
        }

        let boot_vals = [
            Self::BOOTED_TAG_NOT_BOOTED,
            Self::BOOTED_TAG_IS_BOOTED
        ];

        if !boot_vals.iter().any(|t| *t == meta.booted_tagword) {
            defmt::error!("Bad booted tag!");
            return None;
        }

        let key = Key::from_slice(crate::consts::POLY_1305_KEY);
        let mut poly = Poly1305::new(key);

        match meta.image_len_pages {
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

        let uuid_all_zeroes = meta.image_uuid.iter().all(|b| *b == 0);
        let uuid_all_effffs = meta.image_uuid.iter().all(|b| *b == 0xFF);

        if uuid_all_zeroes || uuid_all_effffs {
            defmt::error!("Bad UUID!");
            return None;
        }

        for page in 0..meta.image_len_pages {
            let offset = page * crate::consts::PAGE_SIZE;
            let page_slice = unsafe {
                core::slice::from_raw_parts(
                    meta.app_ptr.as_ptr().add(offset),
                    crate::consts::PAGE_SIZE,
                )
            };

            for chunk in page_slice.chunks_exact(crate::consts::POLY_TAG_SIZE) {
                poly.update(Block::from_slice(chunk));
            }
        }

        let calc_poly: [u8; POLY_TAG_SIZE] = poly.finalize().into_bytes().into();

        let good_poly = calc_poly == meta.image_poly1305_tag;

        if good_poly {
            Some(meta)
        } else {
            defmt::error!("Bad Poly!");
            None
        }

    }
}


#[derive(defmt::Format, Debug, Clone, PartialEq)]
pub enum UsableSections {
    Section1,
    Section2,
    Section3,
}

impl UsableSections {
    pub fn metadata_as_ptr(&self) -> *mut u8 {
        let usize_addr = match self {
            UsableSections::Section1 => crate::consts::SECTION_1_START_METADATA,
            UsableSections::Section2 => crate::consts::SECTION_2_START_METADATA,
            UsableSections::Section3 => crate::consts::SECTION_3_START_METADATA,
        };

        usize_addr as *const u8 as *mut u8
    }

    pub fn app_as_ptr(&self) -> *mut u8 {
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
    pub fn write_word(&mut self, addr: usize, word: u32) {
        if word == 0xFFFF_FFFF {
            return;
        }

        defmt::assert_eq!(addr & 0b11, 0);

        self.wait_ready();

        let mut_ptr = addr as *const u32 as *mut u32;

        unsafe {
            mut_ptr.write_volatile(word);
        }

        cortex_m::asm::dmb();
    }

    #[inline]
    pub fn write_word_app(&mut self, offset_bytes: usize, word: u32) {
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

#[derive(Debug)]
pub enum BootDecision {
    Boot(Bootdata),
    CopyThenFirstBoot {
        source: Metadata,
        boot_seq: u32,
        boot_dat: Bootdata,
    },
    RollbackThenBoot {
        source: Metadata,
        boot_seq: u32,
        boot_dat: Bootdata,
    },
    Halt,
}

pub fn make_decision(
    active_meta: Option<Metadata>,
    a_side_meta: Option<Metadata>,
    b_side_meta: Option<Metadata>,
) -> BootDecision {
    // * Check if a new image was flashed (slot A or slot B)
    //     * If so, validate the image (poly1305, sanity)
    //     * If good, copy image to active

    // NOTE: We only have a flash lifetime of 100_000 cycles,
    // so... I'm not too worried about the rollover condition
    // here.
    let boot_seq = active_meta
        .as_ref()
        .map(|m| m.boot_seq_number)
        .unwrap_or(1)
        .wrapping_add(1);

    if let Some(meta) = a_side_meta.as_ref() {
        let fresh = !meta.has_flashed();
        if fresh {
            let boot_dat = Bootdata {
                app_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section1.metadata_as_ptr())),
                own_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section2.metadata_as_ptr())),
                nxt_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section3.metadata_as_ptr())),
                nxt_image: defmt::unwrap!(NonNull::new(UsableSections::Section3.metadata_as_ptr())),
                is_first_boot: true,
                is_rollback: false,
            };

            return BootDecision::CopyThenFirstBoot {
                source: meta.clone(),
                boot_seq: boot_seq,
                boot_dat,
            };
        }
    }

    if let Some(meta) = b_side_meta.as_ref() {
        let fresh = !meta.has_flashed();
        if fresh {
            let boot_dat = Bootdata {
                app_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section1.metadata_as_ptr())),
                own_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section3.metadata_as_ptr())),
                nxt_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section2.metadata_as_ptr())),
                nxt_image: defmt::unwrap!(NonNull::new(UsableSections::Section2.metadata_as_ptr())),
                is_first_boot: true,
                is_rollback: false,
            };

            return BootDecision::CopyThenFirstBoot {
                source: meta.clone(),
                boot_seq: boot_seq,
                boot_dat,
            };
        }
    }

    let writeover = match (&a_side_meta, &b_side_meta) {
        (None, None) => UsableSections::Section2, // just pick one
        (None, Some(_)) => UsableSections::Section2,
        (Some(_), None) => UsableSections::Section3,
        (Some(a), Some(b)) if a.boot_seq_number > b.boot_seq_number => UsableSections::Section3,
        (Some(_), Some(_)) => UsableSections::Section2,
    };

    let recovery = match (&a_side_meta, &b_side_meta) {
        (Some(a), None) if a.has_flashed() && a.has_booted() => Some(UsableSections::Section2),
        (None, Some(b)) if b.has_flashed() && b.has_booted() => Some(UsableSections::Section3),
        (Some(a), Some(b)) => {
            let a_good = a.has_booted() && a.has_flashed();
            let b_good = b.has_booted() && b.has_flashed();

            match (a_good, b_good) {
                (true, false) => Some(UsableSections::Section2),
                (false, true) => Some(UsableSections::Section3),
                (true, true) => {
                    if a.boot_seq_number > b.boot_seq_number {
                        Some(UsableSections::Section2)
                    } else {
                        Some(UsableSections::Section3)
                    }
                },
                (false, false) => None,
            }
        },
        _ => None,
    };

    if let Some(meta) = active_meta {
        let good = meta.has_booted();
        if good {
            return BootDecision::Boot(Bootdata {
                app_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section1.metadata_as_ptr())),

                // TODO: This is wrong! But for now it's okay because we only touch this on first boot
                // Make this optional?
                own_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section1.metadata_as_ptr())),
                // END TODO

                nxt_metadata: defmt::unwrap!(NonNull::new(writeover.metadata_as_ptr())),
                nxt_image: defmt::unwrap!(NonNull::new(writeover.metadata_as_ptr())),
                is_first_boot: false,
                is_rollback: false,
            })
        }

        if let Some(recovery) = recovery {
            let boot_dat = Bootdata {
                app_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section1.metadata_as_ptr())),
                own_metadata: defmt::unwrap!(NonNull::new(recovery.metadata_as_ptr())),

                // TODO: I'm not *sure* if this is right, but it's okay because we only touch this on
                // first boot.
                nxt_metadata: defmt::unwrap!(NonNull::new(writeover.metadata_as_ptr())),
                nxt_image: defmt::unwrap!(NonNull::new(writeover.metadata_as_ptr())),
                // END TODO

                is_first_boot: false,
                is_rollback: true,
            };

            return BootDecision::CopyThenFirstBoot {
                source: meta.clone(),
                boot_seq: boot_seq,
                boot_dat,
            };
        }
    }

    // TODO: We could try recovering by picking a valid image and booting it, but
    // that would only happen if we managed to erase/corrupt JUST the app page?
    // Maybe worth doing at some point for robustness.
    defmt::error!("Nothing to be done :(");
    BootDecision::Halt
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
