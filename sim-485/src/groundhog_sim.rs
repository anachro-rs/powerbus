use groundhog::RollingTimer;
use once_cell::sync::OnceCell;
use std::{sync::Mutex, time::Instant};

pub struct GlobalRollingTimer {
    start: Instant,
}

impl GlobalRollingTimer {
    pub fn new() -> Self {
        static START: OnceCell<Instant> = OnceCell::new();
        Self {
            start: *START.get_or_init(|| {
                Instant::now()
            }),
        }
    }
}

impl Default for GlobalRollingTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl RollingTimer for GlobalRollingTimer {
    type Tick = u32;

    const TICKS_PER_SECOND: Self::Tick = 1_000_000;

    fn get_ticks(&self) -> Self::Tick {
        (self.start.elapsed().as_micros() & 0xFFFF_FFFF) as Self::Tick
    }

    fn is_initialized(&self) -> bool {
        true
    }
}
