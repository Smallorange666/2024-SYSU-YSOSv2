use boot::{BootInfo, RuntimeServices, Time};

pub struct UefiRuntime {
    runtime_service: &'static RuntimeServices,
}

once_mutex!(UEFI_RUNTIME: UefiRuntime);

impl UefiRuntime {
    pub unsafe fn new(boot_info: &'static BootInfo) -> Self {
        Self {
            runtime_service: boot_info.system_table.runtime_services(),
        }
    }

    pub fn get_time(&self) -> Time {
        self.runtime_service.get_time().unwrap()
    }
}

guard_access_fn!(pub get_uefi_runtime(UEFI_RUNTIME: UefiRuntime));

pub fn init(boot_info: &'static BootInfo) {
    unsafe {
        init_UEFI_RUNTIME(UefiRuntime::new(boot_info));
    }
    println!("[+] UEFI Runtime Initialized.");
}
