use super::LocalApic;
use crate::interrupt::consts::{Interrupts, Irq};
use bit_field::BitField;
use core::fmt::{Debug, Error, Formatter};
use core::ptr::{read_volatile, write_volatile};
use x86::cpuid::CpuId;

enum Registers {
    ID = 0x020,
    VERSION = 0x030,
    TPR = 0x080,
    EOI = 0x0B0,
    SPIV = 0x0F0,
    ESR = 0x280,
    ICRLO = 0x300,
    ICRHI = 0x310,
    LvtTimer = 0x320,
    PMCR = 0x340,
    LvtLINT0 = 0x350,
    LvtLINT1 = 0x360,
    LvtError = 0x370,
    TICR = 0x380,
    TDCR = 0x3E0,
}

use Registers::*;

/// Default physical address of xAPIC
pub const LAPIC_ADDR: u64 = 0xFEE00000;

pub struct XApic {
    addr: u64,
}

impl XApic {
    pub unsafe fn new(addr: u64) -> Self {
        XApic { addr }
    }

    unsafe fn read(&self, reg: Registers) -> u32 {
        read_volatile((self.addr + reg as u64) as *const u32)
    }

    unsafe fn write(&mut self, reg: Registers, value: u32) {
        write_volatile((self.addr + reg as u64) as *mut u32, value);
        self.read(ID);
    }
}

impl LocalApic for XApic {
    /// If this type APIC is supported
    fn support() -> bool {
        // FIXME: Check CPUID to see if xAPIC is supported.
        CpuId::new()
            .get_feature_info()
            .map(|f| f.has_apic())
            .unwrap_or(false)
    }

    /// Initialize the xAPIC for the current CPU.
    fn cpu_init(&mut self) {
        unsafe {
            // FIXME: Enable local APIC; set spurious interrupt vector.
            let mut spiv = self.read(SPIV);
            spiv |= 1 << 8;
            spiv &= !(0xFF);
            spiv |= Interrupts::IrqBase as u32 + Irq::Spurious as u32;
            self.write(SPIV, spiv);

            // FIXME: The timer repeatedly counts down at bus frequency
            self.write(TDCR, 0b1011);

            let mut lvt_timer = self.read(LvtTimer);
            lvt_timer &= !(0xFF);
            lvt_timer |= Interrupts::IrqBase as u32 + Irq::Timer as u32;
            lvt_timer &= !(1 << 16);
            lvt_timer |= 1 << 17;
            self.write(LvtTimer, lvt_timer);

            self.write(TICR, 0x20000);
            // FIXME: Disable logical interrupt lines (LINT0, LINT1)
            self.write(LvtLINT0, 1 << 16);
            self.write(LvtLINT1, 1 << 16);
            // FIXME: Disable performance counter overflow interrupts (PCINT)
            self.write(PMCR, 1 << 16);
            // FIXME: Map error interrupt to IRQ_ERROR.
            let mut irq_error = self.read(LvtError);
            irq_error &= !(0xFF);
            irq_error |= Interrupts::IrqBase as u32 + Irq::Error as u32;
            self.write(LvtError, irq_error);
            // FIXME: Clear error status register (requires back-to-back writes).
            self.write(ESR, 0);
            self.write(ESR, 0);
            // FIXME: Ack any outstanding interrupts.
            self.write(EOI, 0);
            // FIXME: Send an Init Level De-Assert to synchronise arbitration ID's.
            self.write(ICRHI, 0); // set ICR 0x310
            const BCAST: u32 = 1 << 19;
            const INIT: u32 = 5 << 8;
            const TMLV: u32 = 1 << 15; // TM = 1, LV = 0
            self.write(ICRLO, BCAST | INIT | TMLV); // set ICR 0x300
            const DS: u32 = 1 << 12;
            while self.read(ICRLO) & DS != 0 {} // wait for delivery status to clear
                                                //FIXME: Enable interrupts on the APIC (but not on the processor)
            self.write(TPR, 1);
        }

        // NOTE: Try to use bitflags! macro to set the flags.
    }

    fn id(&self) -> u32 {
        // NOTE: Maybe you can handle regs like `0x0300` as a const.
        unsafe { self.read(ID) >> 24 }
    }

    fn version(&self) -> u32 {
        unsafe { self.read(VERSION) }
    }

    fn icr(&self) -> u64 {
        unsafe { (self.read(ICRHI) as u64) << 32 | self.read(ICRLO) as u64 }
    }

    fn set_icr(&mut self, value: u64) {
        unsafe {
            while self.read(ICRLO).get_bit(12) {}
            self.write(ICRHI, (value >> 32) as u32);
            self.write(ICRLO, value as u32);
            while self.read(ICRLO).get_bit(12) {}
        }
    }

    fn eoi(&mut self) {
        unsafe {
            self.write(EOI, 0);
        }
    }
}

impl Debug for XApic {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        f.debug_struct("Xapic")
            .field("id", &self.id())
            .field("version", &self.version())
            .field("icr", &self.icr())
            .finish()
    }
}
