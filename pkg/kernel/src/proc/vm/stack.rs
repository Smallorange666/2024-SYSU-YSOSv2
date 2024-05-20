use x86_64::{
    structures::paging::{mapper::MapToError, page::*, Page},
    VirtAddr,
};

use crate::{
    proc::{processor, KERNEL_PID},
    ProcessId,
};

use super::{FrameAllocatorRef, MapperRef};

use crate::memory::PAGE_SIZE;

// 0xffff_ff00_0000_0000 is the kernel's address space
pub const STACK_MAX: u64 = 0x0000_4000_0000_0000;

pub const STACK_MAX_PAGES: u64 = 0x100000;
pub const STACK_MAX_SIZE: u64 = STACK_MAX_PAGES * PAGE_SIZE;
pub const STACK_START_MASK: u64 = !(STACK_MAX_SIZE - 1);
// [bot..0x2000_0000_0000..top..0x3fff_ffff_ffff]
// init stack
pub const STACK_DEF_PAGE: u64 = 1;
pub const STACK_DEF_SIZE: u64 = STACK_DEF_PAGE * PAGE_SIZE;
pub const STACK_INIT_BOT: u64 = STACK_MAX - STACK_DEF_SIZE;
pub const STACK_INIT_TOP: u64 = STACK_MAX - 8;
// [bot..0xffffff0100000000..top..0xffffff01ffffffff]
// kernel stack
pub const KSTACK_MAX: u64 = 0xffff_ff02_0000_0000;
pub const KSTACK_DEF_PAGE: u64 = 512;
pub const KSTACK_DEF_SIZE: u64 = KSTACK_DEF_PAGE * PAGE_SIZE;
pub const KSTACK_INIT_BOT: u64 = KSTACK_MAX - KSTACK_DEF_SIZE;
pub const KSTACK_INIT_TOP: u64 = KSTACK_MAX - 8;

const STACK_INIT_TOP_PAGE: Page<Size4KiB> = Page::containing_address(VirtAddr::new(STACK_INIT_TOP));

const KSTACK_INIT_PAGE: Page<Size4KiB> = Page::containing_address(VirtAddr::new(KSTACK_INIT_BOT));
const KSTACK_INIT_TOP_PAGE: Page<Size4KiB> =
    Page::containing_address(VirtAddr::new(KSTACK_INIT_TOP));

pub struct Stack {
    range: PageRange<Size4KiB>,
    usage: u64,
}

impl Stack {
    pub fn new(top: Page, size: u64) -> Self {
        Self {
            range: Page::range(top - size + 1, top + 1),
            usage: size,
        }
    }

    pub const fn empty() -> Self {
        Self {
            range: Page::range(STACK_INIT_TOP_PAGE, STACK_INIT_TOP_PAGE),
            usage: 0,
        }
    }

    pub const fn kstack() -> Self {
        Self {
            range: Page::range(KSTACK_INIT_PAGE, KSTACK_INIT_TOP_PAGE),
            usage: KSTACK_DEF_PAGE,
        }
    }

    pub fn usage(&self) -> u64 {
        self.usage
    }

    pub fn bottom(&self) -> VirtAddr {
        self.range.start.start_address()
    }

    pub fn init(
        &mut self,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
        pid: ProcessId,
    ) -> VirtAddr {
        debug_assert!(self.usage == 0, "Stack is not empty.");
        let stack_bottom = STACK_MAX - (pid.0 - 1) as u64 * STACK_MAX_SIZE - STACK_DEF_SIZE;
        info!("Init stack for pid {}: {:#x}", pid.0, stack_bottom);
        self.range = elf::map_pages(stack_bottom, STACK_DEF_PAGE, mapper, alloc, true).unwrap();
        self.usage = STACK_DEF_PAGE;

        self.range.start.start_address() + PAGE_SIZE - 8
    }

    pub fn set_stack(&mut self, bottom: u64, page_num: u64) {
        let start_page = Page::containing_address(VirtAddr::new(bottom));
        self.range = Page::range(start_page, start_page + page_num);
    }

    pub fn stack_offset(&self, old_stack: &Stack) -> u64 {
        let cur_stack_base = self.range.start.start_address().as_u64();
        let old_stack_base = old_stack.range.start.start_address().as_u64();
        let offset = cur_stack_base - old_stack_base;
        debug_assert!(offset % STACK_MAX_SIZE != 0, "Invalid stack offset.");
        offset
    }

    pub fn handle_page_fault(
        &mut self,
        addr: VirtAddr,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
    ) -> bool {
        if !self.is_on_stack(addr) {
            return false;
        }

        if let Err(m) = self.grow_stack(addr, mapper, alloc) {
            error!("Grow stack failed: {:?}", m);
            return false;
        }

        true
    }

    fn is_on_stack(&self, addr: VirtAddr) -> bool {
        let addr = addr.as_u64();
        let cur_stack_bot = self.range.start.start_address().as_u64();
        trace!("Current stack bot: {:#x}", cur_stack_bot);
        trace!("Address to access: {:#x}", addr);
        addr & STACK_START_MASK == cur_stack_bot & STACK_START_MASK
    }

    fn grow_stack(
        &mut self,
        addr: VirtAddr,
        mapper: MapperRef,
        alloc: FrameAllocatorRef,
    ) -> Result<(), MapToError<Size4KiB>> {
        debug_assert!(self.is_on_stack(addr), "Address is not on stack.");

        let new_start_page = Page::containing_address(addr);
        let page_count = self.range.start - new_start_page;

        trace!(
            "Fill missing pages...[{:#x} -> {:#x}) ({} pages)",
            new_start_page.start_address().as_u64(),
            self.range.start.start_address().as_u64(),
            page_count
        );

        let user_access = processor::get_pid() != KERNEL_PID;

        elf::map_pages(
            new_start_page.start_address().as_u64(),
            page_count,
            mapper,
            alloc,
            user_access,
        )?;

        self.range = Page::range(new_start_page, self.range.end);
        self.usage = self.range.count() as u64;

        Ok(())
    }

    pub fn memory_usage(&self) -> u64 {
        self.usage * crate::memory::PAGE_SIZE
    }
}

impl core::fmt::Debug for Stack {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("Stack")
            .field(
                "top",
                &format_args!("{:#x}", self.range.end.start_address().as_u64()),
            )
            .field(
                "bot",
                &format_args!("{:#x}", self.range.start.start_address().as_u64()),
            )
            .finish()
    }
}
