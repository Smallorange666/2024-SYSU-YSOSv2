use core::ptr::copy_nonoverlapping;

use super::ProcessId;
use super::*;
use crate::memory::*;
use crate::proc::paging::PageTableContext;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::sync::Weak;
use alloc::vec::Vec;
use elf::map_range;
use elf::unmap_range;
use spin::*;
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::page::PageRange;
use x86_64::structures::paging::*;
use x86_64::VirtAddr;

#[derive(Clone)]
pub struct Process {
    pid: ProcessId,
    inner: Arc<RwLock<ProcessInner>>,
}

pub struct ProcessInner {
    name: String,
    parent: Option<Weak<Process>>,
    children: Vec<Arc<Process>>,
    ticks_passed: usize,
    status: ProgramStatus,
    exit_code: Option<isize>,
    context: ProcessContext,
    page_table: Option<PageTableContext>,
    proc_data: Option<ProcessData>,
}

impl Process {
    #[inline]
    pub fn pid(&self) -> ProcessId {
        self.pid
    }

    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<ProcessInner> {
        self.inner.write()
    }

    #[inline]
    pub fn read(&self) -> RwLockReadGuard<ProcessInner> {
        self.inner.read()
    }

    pub fn new(
        name: String,
        parent: Option<Weak<Process>>,
        page_table: PageTableContext,
        proc_data: Option<ProcessData>,
    ) -> Arc<Self> {
        let name = name.to_ascii_lowercase();

        // create context
        let pid = ProcessId::new();

        let inner = ProcessInner {
            name,
            parent,
            status: ProgramStatus::Ready,
            context: ProcessContext::default(),
            ticks_passed: 0,
            exit_code: None,
            children: Vec::new(),
            page_table: Some(page_table),
            proc_data: Some(proc_data.unwrap_or_default()),
        };

        trace!("New process {}#{} created.", &inner.name, pid);

        // create process struct
        Arc::new(Self {
            pid,
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    pub fn kill(&self, ret: isize) {
        let mut inner = self.inner.write();

        debug!(
            "Killing process {}#{} with ret code: {}",
            inner.name(),
            self.pid,
            ret
        );

        inner.kill(ret);
    }

    pub fn alloc_init_stack(&self) -> VirtAddr {
        // alloc init stack base on self pid
        self.write().alloc_init_stack(self.pid.0)
    }

    pub fn page_range(&self) -> PageRange {
        self.read().stack_segment.unwrap()
    }

    pub fn cal_page_gap(&self, addr: VirtAddr) -> u64 {
        let nowpage = Page::<Size4KiB>::containing_address(addr);
        self.page_range().start - nowpage
    }

    pub fn page_num(&self) -> u64 {
        self.page_range().end - self.page_range().start
    }

    pub fn enlarge_stack(&self, addr: VirtAddr) {
        let new_page_num = self.cal_page_gap(addr);
        let now_page_num = self.page_num();
        let stack_bottom = STACK_MAX
            - (self.pid.0 - 1) as u64 * STACK_MAX_SIZE
            - (new_page_num + now_page_num) * PAGE_SIZE;

        let mut page_table = self.read().page_table.as_ref().unwrap().mapper();
        let frame_allocator = &mut *get_frame_alloc_for_sure();
        map_range(
            stack_bottom,
            new_page_num,
            &mut page_table,
            frame_allocator,
            true,
        )
        .expect("");

        self.write()
            .proc_data
            .as_mut()
            .unwrap()
            .set_stack(VirtAddr::new(stack_bottom), now_page_num + new_page_num);
    }

    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        // lock inner as write
        let mut inner = self.write();
        // inner fork with parent weak ref
        let child_pid = ProcessId::new();
        let child_inner = inner.fork(Arc::downgrade(self));
        // print the child process info
        trace!(
            "Parent {} forked: {}#{}",
            inner.name,
            child_pid,
            child_inner.name
        );
        // make the arc of child
        let child_proc = Arc::new(Self {
            pid: child_pid,
            inner: Arc::new(RwLock::new(child_inner)),
        });
        // add child to current process's children list
        inner.children.push(child_proc.clone());
        // set fork ret value for parent with `context.set_rax`
        inner.context.set_rax(child_pid.0 as usize);
        // mark the child as ready & return it
        child_proc.write().pause();

        child_proc
    }
}

impl ProcessInner {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tick(&mut self) {
        self.ticks_passed += 1;
    }

    pub fn status(&self) -> ProgramStatus {
        self.status
    }

    pub fn pause(&mut self) {
        self.status = ProgramStatus::Ready;
    }

    pub fn resume(&mut self) {
        self.status = ProgramStatus::Running;
    }

    pub fn block(&mut self) {
        self.status = ProgramStatus::Blocked;
    }

    pub fn exit_code(&self) -> Option<isize> {
        self.exit_code
    }

    pub fn clone_page_table(&self) -> PageTableContext {
        self.page_table.as_ref().unwrap().clone_l4()
    }

    pub fn is_ready(&self) -> bool {
        self.status == ProgramStatus::Ready
    }

    pub fn env(&self, key: &str) -> Option<String> {
        self.proc_data.as_ref().unwrap().env(key)
    }

    pub fn context(&mut self) -> &mut ProcessContext {
        &mut self.context
    }

    /// Save the process's context
    /// mark the process as ready
    pub(super) fn save(&mut self, context: &ProcessContext) {
        // save the process's context
        if self.status != ProgramStatus::Dead {
            self.context.save(context);
            self.pause();
        }
    }

    /// Restore the process's context
    /// mark the process as running
    pub(super) fn restore(&mut self, context: &mut ProcessContext) {
        // restore the process's context
        self.resume();
        self.context.restore(context);
        // restore the process's page table
        self.page_table.as_ref().unwrap().load();
    }

    pub fn parent(&self) -> Option<Arc<Process>> {
        self.parent.as_ref().and_then(|p| p.upgrade())
    }

    pub fn add_child(&mut self, child: Arc<Process>) {
        self.children.push(child);
    }

    pub fn sem_wait(&mut self, key: u32, pid: ProcessId) -> SemaphoreResult {
        self.proc_data.as_mut().unwrap().sem_wait(key, pid)
    }

    pub fn sem_signal(&mut self, key: u32) -> SemaphoreResult {
        self.proc_data.as_mut().unwrap().sem_signal(key)
    }

    pub fn new_sem(&mut self, key: u32, value: usize) -> bool {
        self.proc_data.as_mut().unwrap().new_sem(key, value)
    }

    pub fn remove_sem(&mut self, key: u32) -> bool {
        self.proc_data.as_mut().unwrap().remove_sem(key)
    }

    pub fn kill(&mut self, ret: isize) {
        // set exit code
        self.exit_code = Some(ret);
        // set status to dead
        self.status = ProgramStatus::Dead;

        // take and drop unused resources
        // recycle process stack
        let stack = self.proc_data.as_ref().unwrap().stack_segment.unwrap();
        let frame_allocator = &mut *get_frame_alloc_for_sure();
        let mut page_table = self.page_table.as_ref().unwrap().mapper();
        unmap_range(stack.start, stack.end, &mut page_table, frame_allocator);

        self.page_table.take();
    }

    pub fn alloc_init_stack(&mut self, pid: u16) -> VirtAddr {
        let stack_bottom = STACK_MAX - (pid - 1) as u64 * STACK_MAX_SIZE - STACK_DEF_SIZE;
        let mut page_table = self.page_table.as_ref().unwrap().mapper();
        let frame_allocator = &mut *get_frame_alloc_for_sure();
        trace!("stack_bottom: {:x}", stack_bottom);
        map_range(
            stack_bottom,
            STACK_DEF_PAGE,
            &mut page_table,
            frame_allocator,
            true,
        )
        .expect("");

        let stack_top = VirtAddr::new(stack_bottom + STACK_DEF_SIZE - 8);

        self.proc_data
            .as_mut()
            .unwrap()
            .set_stack(VirtAddr::new(stack_bottom), STACK_DEF_PAGE);

        stack_top
    }

    pub fn init_child_stack(
        &mut self,
        parent: &Weak<Process>,
        child_page_table: &PageTableContext,
    ) -> (u64, u64) {
        let parent = parent.upgrade().unwrap();
        let parent_stack = self.stack_segment.unwrap();
        let count = parent.pid().0 + self.children.len() as u16;
        let mut child_stack_bottom =
            STACK_MAX - (count - 1) as u64 * STACK_MAX_SIZE - STACK_DEF_SIZE;
        let child_stack_count = parent_stack.end - parent_stack.start;
        let frame_allocator = &mut *get_frame_alloc_for_sure();
        while map_range(
            child_stack_bottom,
            child_stack_count,
            &mut child_page_table.mapper(),
            frame_allocator,
            true,
        )
        .is_err()
        {
            trace!("Map child stack to {:#x} failed.", child_stack_bottom);
            child_stack_bottom -= STACK_MAX_SIZE; // stack grow down
        }

        Self::clone_range(
            parent_stack.start.start_address().as_u64(),
            child_stack_bottom,
            child_stack_count as usize,
        );

        (child_stack_bottom, child_stack_count)
    }

    pub fn load_elf(
        &mut self,
        elf: &ElfFile,
        physical_offset: u64,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
        user_access: bool,
    ) -> Result<(), MapToError<Size4KiB>> {
        let mut page_table = self.page_table.as_ref().unwrap().mapper();
        elf::load_elf(
            elf,
            physical_offset,
            &mut page_table,
            frame_allocator,
            user_access,
            &mut self.proc_data.as_mut().unwrap().code_segment_pages,
        )
    }

    pub fn print_info(&self) {
        println!("Process: {}", self.name);
        println!("Ticks: {}", self.ticks_passed);
        let (size, unit) = crate::memory::humanized_size(
            self.proc_data.as_ref().unwrap().code_segment_pages * PAGE_SIZE,
        );
        println!("Code Segment Memory Usage: {:>7.*} {}", 3, size, unit);
        let stack = self.proc_data.as_ref().unwrap().stack_segment.unwrap();
        let (size, unit) = crate::memory::humanized_size((stack.end - stack.start) * PAGE_SIZE);
        println!("Prcoess Memory Usage: {:>7.*} {}", 3, size, unit);
    }

    /// Clone a range of memory
    ///
    /// - `src_addr`: the address of the source memory
    /// - `dest_addr`: the address of the target memory
    /// - `size`: the count of pages to be cloned
    fn clone_range(src_addr: u64, dest_addr: u64, size: usize) {
        trace!("Clone range: {:#x} -> {:#x}", src_addr, dest_addr);
        unsafe {
            copy_nonoverlapping::<u8>(
                src_addr as *mut u8,
                dest_addr as *mut u8,
                size * Size4KiB::SIZE as usize,
            );
        }
    }
    pub fn fork(&mut self, parent: Weak<Process>) -> ProcessInner {
        // clone the process data struct
        let mut child_proc_data = self.proc_data.as_ref().unwrap().clone();
        // clone the page table context (see instructions)
        let child_page_table = self.page_table.as_ref().unwrap().fork();
        // alloc & map new stack for child (see instructions)
        // copy the *entire stack* from parent to child
        let (child_stack_bottom, child_stack_count) =
            self.init_child_stack(&parent, &child_page_table);
        // update child's stack frame
        let mut child_context = self.context;
        let child_stack_top =
            (self.context.stack_top() & 0xFFFFFFFF) | child_stack_bottom & !(0xFFFFFFFF);
        child_context.update_stack_frame(VirtAddr::new(child_stack_top));
        child_proc_data.set_stack(VirtAddr::new(child_stack_bottom), child_stack_count);
        // set the return value 0 for child with `context.set_rax`
        child_context.set_rax(0);
        // construct the child process inner
        ProcessInner {
            name: self.name.clone(),
            parent: Some(parent),
            children: Vec::new(),
            ticks_passed: 0,
            status: ProgramStatus::Ready,
            exit_code: None,
            context: child_context,
            page_table: Some(child_page_table),
            proc_data: Some(child_proc_data),
        }
    }

    pub fn open_file(&mut self, path: &str) -> u8 {
        self.proc_data.as_mut().unwrap().open_file(path)
    }

    pub fn close_file(&mut self, fd: u8) -> bool {
        self.proc_data.as_mut().unwrap().close_file(fd)
    }
}

impl core::ops::Deref for Process {
    type Target = Arc<RwLock<ProcessInner>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl core::ops::Deref for ProcessInner {
    type Target = ProcessData;

    fn deref(&self) -> &Self::Target {
        self.proc_data
            .as_ref()
            .expect("Process data empty. The process may be killed.")
    }
}

impl core::ops::DerefMut for ProcessInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.proc_data
            .as_mut()
            .expect("Process data empty. The process may be killed.")
    }
}

impl core::fmt::Debug for Process {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let mut f = f.debug_struct("Process");
        f.field("pid", &self.pid);

        let inner = self.inner.read();
        f.field("name", &inner.name);
        f.field("parent", &inner.parent().map(|p| p.pid));
        f.field("status", &inner.status);
        f.field("ticks_passed", &inner.ticks_passed);
        f.field(
            "children",
            &inner.children.iter().map(|c| c.pid.0).collect::<Vec<u16>>(),
        );
        f.field("page_table", &inner.page_table);
        f.field("status", &inner.status);
        f.field("context", &inner.context);
        f.field("stack", &inner.proc_data.as_ref().map(|d| d.stack_segment));
        f.finish()
    }
}

impl core::fmt::Display for Process {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let inner = self.inner.read();
        write!(
            f,
            " #{:-3} | #{:-3} | {:12} | {:7} | {:?}",
            self.pid.0,
            inner.parent().map(|p| p.pid.0).unwrap_or(0),
            inner.name,
            inner.ticks_passed,
            inner.status
        )?;
        Ok(())
    }
}
