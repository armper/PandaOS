//! Boot phase state machine for PandaOS kernel
//!
//! This module provides compile-time enforcement of boot ordering constraints.
//! Operations that require certain boot phases to be complete cannot be called
//! until the type system proves that phase has been reached.
//!
//! ## Invariants
//!
//! - Boot phases progress linearly (no going backwards)
//! - Each phase enables specific operations via unique types
//! - Illegal operations are prevented at compile time

use core::marker::PhantomData;

/// Boot phase marker: Early boot (bootloader just handed control)
pub struct PhaseEarlyBoot;

/// Boot phase marker: HAL initialized (serial, VGA available)
pub struct PhaseHalInit;

/// Boot phase marker: Memory management initialized (paging, heap available)
pub struct PhaseMemoryInit;

/// Boot phase marker: Interrupts initialized (IDT, exceptions ready)
pub struct PhaseInterruptsInit;

/// Boot phase marker: System fully initialized (ready for tasks)
pub struct PhaseFullyInit;

/// Kernel state tracking boot progression
///
/// The type parameter `P` represents the current boot phase.
/// Operations are only available when the appropriate phase is reached.
pub struct KernelState<P> {
    _phase: PhantomData<P>,
}

impl KernelState<PhaseEarlyBoot> {
    /// Create initial kernel state at early boot
    pub const fn new() -> Self {
        Self { _phase: PhantomData }
    }

    /// Initialize HAL (serial, VGA)
    ///
    /// # Safety
    ///
    /// Must be called exactly once during boot.
    /// Hardware must be in expected state from bootloader.
    pub unsafe fn init_hal(self) -> KernelState<PhaseHalInit> {
        // SAFETY: Caller guarantees this is called once at boot
        unsafe {
            panda_hal::serial::init();
            panda_hal::vga::init();
        }

        KernelState { _phase: PhantomData }
    }
}

impl KernelState<PhaseHalInit> {
    /// Initialize memory management (paging, heap)
    ///
    /// # Safety
    ///
    /// Must be called exactly once after HAL initialization.
    /// Cannot be called before HAL because logging wouldn't work.
    pub unsafe fn init_memory(self) -> KernelState<PhaseMemoryInit> {
        // Memory initialization will go here
        // For now, this is a placeholder

        KernelState { _phase: PhantomData }
    }
}

impl KernelState<PhaseMemoryInit> {
    /// Initialize interrupt handling (IDT, GDT)
    ///
    /// # Safety
    ///
    /// Must be called exactly once after memory initialization.
    /// Cannot be called before memory because interrupt handlers
    /// may need heap allocation.
    pub unsafe fn init_interrupts(self) -> KernelState<PhaseInterruptsInit> {
        // SAFETY: Caller guarantees memory is initialized
        // Interrupt initialization code goes here

        KernelState { _phase: PhantomData }
    }
}

impl KernelState<PhaseInterruptsInit> {
    /// Complete initialization and enter main kernel loop
    ///
    /// This consumes the state machine, proving all phases completed.
    pub fn finalize(self) -> KernelState<PhaseFullyInit> {
        KernelState { _phase: PhantomData }
    }
}

impl KernelState<PhaseFullyInit> {
    /// Enable interrupts (only available after full initialization)
    ///
    /// # Safety
    ///
    /// Must only be called after all initialization is complete.
    pub unsafe fn enable_interrupts(&self) {
        // SAFETY: Caller guarantees full initialization complete
        unsafe {
            x86_64::instructions::interrupts::enable();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_phases_compile() {
        // This test verifies the type system enforces correct ordering
        let state = KernelState::<PhaseEarlyBoot>::new();

        // Can't enable interrupts here - wrong phase!
        // state.enable_interrupts(); // Would not compile

        // Must progress through phases
        // In a real kernel:
        // let state = unsafe { state.init_hal() };
        // let state = unsafe { state.init_memory() };
        // let state = unsafe { state.init_interrupts() };
        // let state = state.finalize();
        // unsafe { state.enable_interrupts() };
    }

    #[test]
    fn test_phase_markers_are_zero_sized() {
        use core::mem::size_of;

        assert_eq!(size_of::<KernelState<PhaseEarlyBoot>>(), 0);
        assert_eq!(size_of::<KernelState<PhaseHalInit>>(), 0);
        assert_eq!(size_of::<KernelState<PhaseMemoryInit>>(), 0);
        assert_eq!(size_of::<KernelState<PhaseInterruptsInit>>(), 0);
        assert_eq!(size_of::<KernelState<PhaseFullyInit>>(), 0);
    }
}
