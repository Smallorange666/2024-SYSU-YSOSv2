use core::sync::atomic::{AtomicU64, Ordering};

use alloc::sync::Arc;
use x86_64::{
    structures::paging::{mapper::UnmapError, Page, Size4KiB},
    VirtAddr,
};

use crate::{get_pid, proc::vm::PAGE_SIZE, KERNEL_PID};

use super::{FrameAllocatorRef, MapperRef};

// user process runtime heap
// 0x100000000 bytes -> 4GiB
// from 0x0000_2000_0000_0000 to 0x0000_2000_ffff_fff8
pub const HEAP_START: u64 = 0x2000_0000_0000;
pub const HEAP_PAGES: u64 = 0x100000;
pub const HEAP_SIZE: u64 = HEAP_PAGES * crate::memory::PAGE_SIZE;
pub const HEAP_END: u64 = HEAP_START + HEAP_SIZE - 8;

/// User process runtime heap
///
/// always page aligned, the range is [base, end)
pub struct Heap {
    /// the base address of the heap
    ///
    /// immutable after initialization
    base: VirtAddr,

    /// the current end address of the heap
    ///
    /// use atomic to allow multiple threads to access the heap
    end: Arc<AtomicU64>,
}

impl Heap {
    pub fn empty() -> Self {
        Self {
            base: VirtAddr::new(HEAP_START),
            end: Arc::new(AtomicU64::new(HEAP_START)),
        }
    }

    pub fn fork(&self) -> Self {
        Self {
            base: self.base,
            end: self.end.clone(),
        }
    }

    pub fn brk(
        &self,
        addr: Option<VirtAddr>,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
    ) -> Option<VirtAddr> {
        let now_end = VirtAddr::new(self.end.load(Ordering::SeqCst));
        let upper_bound = (now_end.as_u64() + 8) & (!0xfff);
        // FIXME: if new_end is None, return the current end address
        if let Some(new_end) = addr {
            if new_end.as_u64() >= HEAP_START && new_end.as_u64() <= HEAP_END {
                let new_upper_bound = (Page::<Size4KiB>::containing_address(new_end) + 1)
                    .start_address()
                    .as_u64();

                if new_end == self.base {
                    if let Ok(_) = self.clean_up(mapper, alloc) {
                        Some(self.base)
                    } else {
                        None
                    }
                } else if new_upper_bound == upper_bound {
                    Some(now_end)
                } else if new_upper_bound > upper_bound {
                    let pages = (new_upper_bound - upper_bound) / PAGE_SIZE;
                    if let Ok(_) =
                        elf::map_pages(upper_bound, pages, mapper, alloc, get_pid() != KERNEL_PID)
                    {
                        self.end.swap(new_end.as_u64(), Ordering::SeqCst);
                        Some(new_end)
                    } else {
                        None
                    }
                } else {
                    let pages = (upper_bound - new_upper_bound) / PAGE_SIZE;
                    if let Ok(_) = elf::unmap_pages(new_upper_bound, pages, mapper, alloc, true) {
                        self.end.swap(new_end.as_u64(), Ordering::SeqCst);
                        Some(new_end)
                    } else {
                        None
                    }
                }
            } else {
                None
            }
        } else {
            Some(now_end)
        }

        // FIXME: check if the new_end is valid (in range [base, base + HEAP_SIZE])

        // FIXME: calculate the difference between the current end and the new end

        // NOTE: print the heap difference for debugging

        // FIXME: do the actual mapping or unmapping

        // FIXME: update the end address
    }

    pub(super) fn clean_up(
        &self,
        mapper: MapperRef,
        dealloc: FrameAllocatorRef,
    ) -> Result<(), UnmapError> {
        if self.memory_usage() == 0 {
            return Ok(());
        }
        // FIXME: load the current end address and **reset it to base** (use `swap`)
        let origin_end = self.end.swap(self.base.as_u64(), Ordering::SeqCst);

        let pages = Page::<Size4KiB>::containing_address(VirtAddr::new(origin_end))
            - Page::containing_address(self.base);

        // FIXME: unmap the heap pages
        if origin_end == self.base.as_u64() {
            Ok(())
        } else {
            elf::unmap_pages(self.base.as_u64(), pages, mapper, dealloc, true)
        }
    }

    pub fn memory_usage(&self) -> u64 {
        self.end.load(Ordering::Relaxed) - self.base.as_u64()
    }
}

impl core::fmt::Debug for Heap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Heap")
            .field("base", &format_args!("{:#x}", self.base.as_u64()))
            .field(
                "end",
                &format_args!("{:#x}", self.end.load(Ordering::Relaxed)),
            )
            .finish()
    }
}
