use core::ptr::NonNull;

use crate::consts::PAGE_SIZE;

#[derive(Debug)]
pub struct Bootdata {
    pub app_metadata: NonNull<u8>,
    pub own_metadata: NonNull<u8>,
    pub nxt_metadata: NonNull<u8>,
    pub nxt_image: NonNull<u8>,
    pub is_first_boot: bool,
    pub is_rollback: bool,
}

impl Bootdata {
    pub fn write_to(addr: usize) -> Result<(), ()> {
        if addr & 0b11 != 0 {
            return Err(());
        }

        todo!()
    }

    pub fn load_from(addr: usize) -> Option<Self> {
        if addr & 0b11 != 0 {
            return None;
        }

        let base = addr as *const u8;

        let magic_1;
        let app_metadata;
        let own_metadata;
        let nxt_metadata;
        let nxt_image;

        let mut _unused;

        let is_first_boot;
        let is_rollback;

        let magic_2;

        unsafe {
            magic_1 = base.add(0).cast::<u32>().read_volatile();
            app_metadata = base.add(4).cast::<u32>().read_volatile();
            own_metadata = base.add(8).cast::<u32>().read_volatile();
            nxt_metadata = base.add(12).cast::<u32>().read_volatile();
            nxt_image = base.add(16).cast::<u32>().read_volatile();

            _unused = base.add(20).cast::<u8>().read_volatile();
            _unused = base.add(21).cast::<u8>().read_volatile();

            is_first_boot = base.add(22).cast::<u8>().read_volatile();
            is_rollback = base.add(23).cast::<u8>().read_volatile();

            magic_2 = base.add(24).cast::<u32>().read_volatile();
        }

        if magic_1 != 0xB007DA7A {
            return None;
        }

        if magic_2 != 0xB007DA7A {
            return None;
        }

        let app_metadata = nn_page_aligned(app_metadata)?;
        let own_metadata = nn_page_aligned(own_metadata)?;
        let nxt_metadata = nn_page_aligned(nxt_metadata)?;
        let nxt_image = nn_page_aligned(nxt_image)?;

        let is_first_boot = bool_check(is_first_boot)?;
        let is_rollback = bool_check(is_rollback)?;

        Some(Self {
            app_metadata,
            own_metadata,
            nxt_metadata,
            nxt_image,
            is_first_boot,
            is_rollback,
        })
    }
}

fn bool_check(input: u8) -> Option<bool> {
    match input {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn nn_page_aligned(input: u32) -> Option<NonNull<u8>> {
    if input & (PAGE_SIZE as u32 - 1) != 0 {
        None
    } else {
        Some(NonNull::new(input as *const u8 as *mut u8)?)
    }
}

// LAYOUT
//
// * 0000: Magic word: 0xB007DA7A
// * 0004: App metadata ptr (active image)
// * 0008: Own metadata ptr (storage)
// * 000C: Next Metadata ptr (storage - to flash)
// * 0010: Next App slot ptr (storage - to flash)
// * 0014: Is first boot?
// * 0015: Is rollback boot?
// * 0016: Unused
// * 0017: Unused
// * 0018: Magic word: 0xB007DA7A
