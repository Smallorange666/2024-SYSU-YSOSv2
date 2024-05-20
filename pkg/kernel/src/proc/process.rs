use core::ptr::copy_nonoverlapping;

use super::ProcessId;
use super::*;
use crate::memory::*;
use crate::proc::paging::PageTableContext;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::sync::Weak;
use alloc::vec::Vec;
use elf::map_pages;
use spin::*;
use vm::*;
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
    proc_data: Option<ProcessData>,
    proc_vm: Option<ProcessVm>,
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
        proc_vm: Option<ProcessVm>,
        proc_data: Option<ProcessData>,
    ) -> Arc<Self> {
        let name = name.to_ascii_lowercase();

        // create context
        let pid = ProcessId::new();
        let proc_vm = proc_vm.unwrap_or_else(|| ProcessVm::new(PageTableContext::new()));

        let inner = ProcessInner {
            name,
            parent,
            status: ProgramStatus::Ready,
            context: ProcessContext::default(),
            ticks_passed: 0,
            exit_code: None,
            children: Vec::new(),
            proc_vm: Some(proc_vm),
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

    pub fn vm(&self) -> &ProcessVm {
        self.proc_vm.as_ref().unwrap()
    }

    pub fn vm_mut(&mut self) -> &mut ProcessVm {
        self.proc_vm.as_mut().unwrap()
    }

    pub fn handle_page_fault(&mut self, addr: VirtAddr) -> bool {
        self.vm_mut().handle_page_fault(addr)
    }

    pub fn clone_page_table(&self) -> PageTableContext {
        self.vm().page_table.clone_l4()
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
        self.vm().page_table.load();
    }

    pub fn init_stack_frame(&mut self, entry: VirtAddr, stack_top: VirtAddr) {
        self.context.init_stack_frame(entry, stack_top)
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
        self.proc_vm.take();
        self.proc_data.take();
    }

    pub fn alloc_init_stack(&mut self, pid: u16) -> VirtAddr {
        let mut page_table = self.vm().page_table.mapper();
        let frame_allocator = &mut *get_frame_alloc_for_sure();
        let stack_bottom = STACK_MAX - (pid - 1) as u64 * STACK_MAX_SIZE - STACK_DEF_SIZE;
        trace!("stack_bottom: {:x}", stack_bottom);
        self.vm_mut()
            .stack
            .init(&mut page_table, frame_allocator, ProcessId(pid));
        info!("2");
        VirtAddr::new(stack_bottom + STACK_DEF_SIZE - 8)
    }

    pub fn init_child_stack(
        &mut self,
        parent: &Weak<Process>,
        child_page_table: &PageTableContext,
    ) -> (u64, u64) {
        let parent = parent.upgrade().unwrap();
        let parent_stack = &self.vm().stack;
        let count = parent.pid().0 + self.children.len() as u16;
        let mut child_stack_bottom =
            STACK_MAX - (count - 1) as u64 * STACK_MAX_SIZE - STACK_DEF_SIZE;
        let frame_allocator = &mut *get_frame_alloc_for_sure();
        let child_stack_count = parent_stack.usage();
        while map_pages(
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
            parent_stack.bottom().as_u64(),
            child_stack_bottom,
            child_stack_count as usize,
        );

        (child_stack_bottom, child_stack_count)
    }

    pub fn load_elf(&mut self, elf: &ElfFile, pid: ProcessId) -> VirtAddr {
        self.vm_mut().load_elf(elf, pid)
    }

    pub fn print_info(&self) {
        println!("Process: {}", self.name);
        println!("Ticks: {}", self.ticks_passed);
        let (size, unit) =
            crate::humanized_size(self.proc_data.as_ref().unwrap().code_segment_pages * PAGE_SIZE);
        println!("Code Segment Memory Usage: {:>7.*} {}", 3, size, unit);
        let (size, unit) = crate::humanized_size(self.vm().stack.usage() * PAGE_SIZE);
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
        let child_proc_data = self.proc_data.as_ref().unwrap().clone();

        // clone the page table context (see instructions)
        let child_page_table = self.vm().page_table.fork();

        // alloc & map new stack for child (see instructions)
        // copy the *entire stack* from parent to child
        let (child_stack_bottom, child_stack_count) =
            self.init_child_stack(&parent, &child_page_table);

        // update child's stack frame
        let mut child_context = self.context;
        let child_stack_top =
            (self.context.stack_top() & 0xFFFFFFFF) | child_stack_bottom & !(0xFFFFFFFF);
        child_context.update_stack_frame(VirtAddr::new(child_stack_top));

        let mut proc_vm = ProcessVm::new(child_page_table);
        proc_vm.stack = Stack::new(
            Page::containing_address(VirtAddr::new(child_stack_top)),
            child_stack_count,
        );

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
            proc_vm: Some(proc_vm),
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
        let inner = self.inner.read();
        f.debug_struct("Process")
            .field("pid", &self.pid)
            .field("name", &inner.name)
            .field("parent", &inner.parent().map(|p| p.pid))
            .field("status", &inner.status)
            .field("ticks_passed", &inner.ticks_passed)
            .field("children", &inner.children.iter().map(|c| c.pid.0))
            .field("status", &inner.status)
            .field("context", &inner.context)
            .field("vm", &inner.proc_vm)
            .finish()
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
