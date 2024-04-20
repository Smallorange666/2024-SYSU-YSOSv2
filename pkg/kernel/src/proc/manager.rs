use crate::memory::{get_frame_alloc_for_sure, PHYSICAL_OFFSET};

use super::*;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Weak;
use alloc::{collections::VecDeque, format, sync::Arc};
use spin::mutex::Mutex;
use spin::RwLock;
use x86_64::VirtAddr;

pub static PROCESS_MANAGER: spin::Once<ProcessManager> = spin::Once::new();

pub fn init(init: Arc<Process>, app_list: boot::AppListRef) {
    // set init process as Running
    init.write().resume();
    // set processor's current pid to init's pid
    processor::set_pid(init.pid());

    PROCESS_MANAGER.call_once(|| ProcessManager::new(init, app_list));
}

pub fn get_process_manager() -> &'static ProcessManager {
    PROCESS_MANAGER
        .get()
        .expect("Process Manager has not been initialized")
}

pub struct ProcessManager {
    processes: RwLock<BTreeMap<ProcessId, Arc<Process>>>,
    ready_queue: Mutex<VecDeque<ProcessId>>,
    waiting_processes: Mutex<BTreeMap<ProcessId, BTreeSet<ProcessId>>>,
    app_list: boot::AppListRef,
}

impl ProcessManager {
    pub fn new(init: Arc<Process>, app_list: boot::AppListRef) -> Self {
        let mut processes = BTreeMap::new();
        let ready_queue = VecDeque::new();
        let waiting_processes = BTreeMap::new();
        let pid = init.pid();

        trace!("Init {:#?}", init);

        processes.insert(pid, init);
        Self {
            processes: RwLock::new(processes),
            ready_queue: Mutex::new(ready_queue),
            waiting_processes: Mutex::new(waiting_processes),
            app_list,
        }
    }

    #[inline]
    pub fn push_ready(&self, pid: ProcessId) {
        self.ready_queue.lock().push_back(pid);
    }

    #[inline]
    pub fn add_waiting(&self, pid: ProcessId) {
        self.waiting_processes
            .lock()
            .entry(pid)
            .or_default()
            .insert(get_pid());
    }

    #[inline]
    fn add_proc(&self, pid: ProcessId, proc: Arc<Process>) {
        self.processes.write().insert(pid, proc);
    }

    #[inline]
    fn get_proc(&self, pid: &ProcessId) -> Option<Arc<Process>> {
        self.processes.read().get(pid).cloned()
    }

    #[inline]
    pub fn block_proc(&self, pid: &ProcessId) {
        self.get_proc(pid).unwrap().write().block();
    }
    pub fn current(&self) -> Arc<Process> {
        self.get_proc(&processor::get_pid())
            .expect("No current process")
    }

    pub fn wake_up(&self, pid: ProcessId) {
        self.get_proc(&pid).unwrap().write().pause();
        self.push_ready(pid);
    }

    pub fn wake_waiting(&self, ret: isize) {
        let now_pid = get_pid();
        let mut wait_proc = self.waiting_processes.lock();
        if let Some(wait_set) = wait_proc.remove(&now_pid) {
            for pid in wait_set {
                self.get_proc(&pid)
                    .unwrap()
                    .write()
                    .context()
                    .set_rax(ret as usize);
                self.wake_up(pid);
            }
        }
    }

    pub fn get_exit_code(&self, pid: ProcessId) -> Option<isize> {
        self.get_proc(&pid).unwrap().read().exit_code()
    }

    pub fn app_list(&self) -> boot::AppListRef {
        self.app_list
    }

    pub fn spawn(
        &self,
        elf: &ElfFile,
        name: String,
        parent: Option<Weak<Process>>,
        proc_data: Option<ProcessData>,
    ) -> ProcessId {
        let kproc = self.get_proc(&KERNEL_PID).unwrap();
        let page_table = kproc.read().clone_page_table();
        let proc = Process::new(name, parent, page_table, proc_data);
        let pid = proc.pid();
        let mut inner = proc.write();

        // load elf to process pagetable
        inner
            .load_elf(
                elf,
                *PHYSICAL_OFFSET.get().unwrap(),
                &mut *get_frame_alloc_for_sure(),
                true,
            )
            .expect("");
        drop(inner);

        // alloc new stack for process
        let stack_top = proc.alloc_init_stack();
        trace!("entry: {:x}", elf.header.pt2.entry_point());
        let entry = VirtAddr::new(elf.header.pt2.entry_point());
        proc.write().context().init_stack_frame(entry, stack_top);

        // mark process as ready
        proc.write().pause();
        trace!("New {:#?}", &proc);

        // something like kernel thread
        self.add_proc(proc.pid(), proc.clone());
        self.push_ready(proc.pid());

        pid
    }

    pub fn save_current(&self, context: &ProcessContext) -> ProcessId {
        // save now current into process context
        let temp = self.current();
        let mut nowproc = temp.write();
        // update current process's tick count
        nowproc.tick();
        // update current process's context
        nowproc.save(context);
        // push current process to ready queue if still alive
        temp.pid()
    }

    pub fn switch_next(&self, context: &mut ProcessContext) -> ProcessId {
        // fetch the next process from ready queue
        let mut nextpid = self.ready_queue.lock().pop_front().unwrap();
        let mut nextproc = self.get_proc(&nextpid).unwrap();
        // check if the next process is ready, continue to fetch if not ready
        while !nextproc.read().is_ready() {
            self.push_ready(nextpid);
            nextpid = self.ready_queue.lock().pop_front().unwrap();
            nextproc = self.get_proc(&nextpid).unwrap();
        }
        // restore next process's context
        nextproc.write().restore(context);
        // update processor's current pid
        processor::set_pid(nextpid);

        nextpid
    }

    pub fn kill_current(&self, ret: isize) {
        self.kill(processor::get_pid(), ret);
    }

    pub fn handle_page_fault(&self, addr: VirtAddr, err_code: PageFaultErrorCode) -> bool {
        // handle page fault
        let nowproc = self.current();
        if !nowproc.read().is_on_stack(addr)
            || err_code.contains(PageFaultErrorCode::PROTECTION_VIOLATION)
        {
            false
        } else {
            nowproc.enlarge_stack(addr);
            true
        }
    }

    pub fn kill_self(&self, ret: isize) {
        self.kill(processor::get_pid(), ret);
    }

    pub fn kill(&self, pid: ProcessId, ret: isize) {
        let proc = self.get_proc(&pid);

        if proc.is_none() {
            warn!("Process #{} not found.", pid);
            return;
        }

        let proc = proc.unwrap();

        if proc.read().status() == ProgramStatus::Dead {
            warn!("Process #{} is already dead.", pid);
            return;
        }

        trace!("Kill Porcess {:?}", pid);

        proc.kill(ret);
    }

    pub fn print_process_list(&self) {
        let mut output = String::from("  PID | PPID | Process Name |  Ticks  | Status \n");

        for (_, p) in self.processes.read().iter() {
            if p.read().status() != ProgramStatus::Dead {
                output += format!("{}\n", p).as_str();
            }
        }

        // TODO: print memory usage of kernel heap

        output += format!("Queue  : {:?}\n", self.ready_queue.lock()).as_str();

        output += &processor::print_processors();

        print!("{}", output);
    }

    pub fn print_process_info(&self, pid: &ProcessId) -> bool {
        if let Some(proc) = self.get_proc(pid) {
            proc.read().print_info();
            true
        } else {
            warn!("Process #{} not found.", pid);
            false
        }
    }

    pub fn is_proc_alive(&self, pid: &ProcessId) -> bool {
        if let Some(proc) = self.get_proc(pid) {
            proc.read().status() != ProgramStatus::Dead
        } else {
            false
        }
    }

    pub fn fork(&self) -> Arc<Process> {
        // get current process
        let proc = self.current();
        // fork to get child
        let child = proc.fork();
        // add child to process list
        self.add_proc(child.pid(), child.clone());
        // maybe print the process ready queue?
        debug!("Ready Queue: {:?}", self.ready_queue.lock());

        child
    }

    pub fn read(&self, fd: u8, buf: &mut [u8]) -> isize {
        self.current().read().read(fd, buf)
    }

    pub fn write(&self, fd: u8, buf: &[u8]) -> isize {
        self.current().write().write(fd, buf)
    }

    pub fn open_file(&self, path: &str) -> u8 {
        self.current().write().open_file(path)
    }

    pub fn close_file(&self, fd: u8) -> bool {
        self.current().write().close_file(fd)
    }

    pub fn cat(&self, fd: u8) {
        self.current().write().cat(fd)
    }
}
