use core::ops::Range;

use x86_64::PhysAddr;

#[repr(transparent)]
#[derive(Clone)]
pub struct Frame(u64);

impl Frame {
    pub const fn from_index(index: u64) -> Self {
        Self { 0: index }
    }

    pub fn from_addr(addr: u64) -> Self {
        assert_eq!(
            addr & !0x000FFFFF_FFFFF000,
            0,
            "frame address format is invalid: {}",
            addr
        );
        Self { 0: addr / 0x1000 }
    }

    pub const fn index(&self) -> u64 {
        self.0
    }

    pub fn addr(&self) -> PhysAddr {
        PhysAddr::new(self.0 * 0x1000)
    }

    pub unsafe fn clear(&mut self) {
        core::ptr::write_bytes((self.0 * 0x1000) as *mut u8, 0x0, 0x1000);
    }

    pub fn range(range: Range<u64>) -> FrameIterator {
        FrameIterator {
            current: Frame::from_addr(range.start),
            end: Frame::from_addr(range.end),
        }
    }
}

pub struct FrameIterator {
    current: Frame,
    end: Frame,
}

impl Iterator for FrameIterator {
    type Item = Frame;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.0 < self.end.0 {
            let frame = self.current.clone();
            self.current.0 += 1;
            Some(frame)
        } else {
            None
        }
    }
}

impl core::fmt::Debug for Frame {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_tuple("Frame").field(&self.addr()).finish()
    }
}
