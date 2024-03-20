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

bitflags! {
    struct ApicRegisters:u32{
        const MASK=0b0000_0000_0000_0001_0000_0000_0000_0000;
        const BCAST= 0b0000_0000_0000_1000_0000_0000_0000_0000;
        const INIT = 0b0000_0000_0000_0000_0000_0101_0000_0000;
        const TMLV= 0b0000_0000_0000_0000_1000_0000_0000_0000;
        const DS = 0b0000_0000_0000_0000_0001_0000_0000_0000;
        const SPI_VECTOR=Interrupts::IrqBase as u32 + Irq::Spurious as u32;
        const TIMER=Interrupts::IrqBase as u32 + Irq::Timer as u32;
        const ERROR=Interrupts::IrqBase as u32 + Irq::Error as u32;
    }
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
        // Check CPUID to see if xAPIC is supported.
        CpuId::new()
            .get_feature_info()
            .map(|f| f.has_apic())
            .unwrap_or(false)
    }

    /// Initialize the xAPIC for the current CPU.
    fn cpu_init(&mut self) {
        unsafe {
            // Enable local APIC; set spurious interrupt vector.
            let mut spiv = self.read(SPIV);
            spiv.set_bit(8, true);
            spiv.set_bits(0..8, ApicRegisters::SPI_VECTOR.bits());
            self.write(SPIV, spiv);

            // The timer repeatedly counts down at bus frequency
            self.write(TDCR, 0b1011);

            let mut lvt_timer = self.read(LvtTimer);
            lvt_timer.set_bits(0..8, ApicRegisters::TIMER.bits());
            lvt_timer.set_bit(16, false);
            lvt_timer.set_bit(17, true);
            self.write(LvtTimer, lvt_timer);

            self.write(TICR, 0x20000);
            // Disable logical interrupt lines (LINT0, LINT1) and performance counter overflow interrupts (PCINT)
            self.write(LvtLINT0, ApicRegisters::MASK.bits());
            self.write(LvtLINT1, ApicRegisters::MASK.bits());
            self.write(PMCR, ApicRegisters::MASK.bits());
            // Map error interrupt to IRQ_ERROR.
            let mut irq_error = self.read(LvtError);
            irq_error.set_bits(0..8, ApicRegisters::ERROR.bits());
            self.write(LvtError, irq_error);
            // Clear error status register (requires back-to-back writes).
            self.write(ESR, 0);
            self.write(ESR, 0);
            // Ack any outstanding interrupts.
            self.write(EOI, 0);
            // Send an Init Level De-Assert to synchronise arbitration ID's.
            self.write(ICRHI, 0); // set ICR 0x310
            self.write(
                ICRLO,
                (ApicRegisters::BCAST | ApicRegisters::INIT | ApicRegisters::TMLV).bits(),
            ); // set ICR 0x300

            // wait for delivery status to clear
            while ApicRegisters::from_bits(self.read(ICRLO))
                .unwrap()
                .contains(ApicRegisters::DS)
            {}
            // Enable interrupts on the APIC (but not on the processor)
            self.write(TPR, 1);
        }
    }

    fn id(&self) -> u32 {
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
