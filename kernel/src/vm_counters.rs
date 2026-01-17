//! Virtual memory debug counters
//!
//! This module provides optional counters for tracking demand paging,
//! COW faults, and other VM events. Counters are only enabled when
//! compiled with debug_assertions or specific features.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Global page fault counter
static PAGE_FAULTS_TOTAL: AtomicUsize = AtomicUsize::new(0);

/// Demand allocation counter (pages allocated on first access)
static DEMAND_ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

/// COW fault counter (write to COW page triggering copy)
static COW_FAULTS: AtomicUsize = AtomicUsize::new(0);

/// Frames shared counter (frames with refcount > 1)
static FRAMES_SHARED: AtomicUsize = AtomicUsize::new(0);

/// Increment page fault counter
pub fn inc_page_faults() {
    PAGE_FAULTS_TOTAL.fetch_add(1, Ordering::Relaxed);
}

/// Increment demand allocation counter
pub fn inc_demand_allocations() {
    DEMAND_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
}

/// Increment COW fault counter
pub fn inc_cow_faults() {
    COW_FAULTS.fetch_add(1, Ordering::Relaxed);
}

/// Set frames shared counter
pub fn set_frames_shared(count: usize) {
    FRAMES_SHARED.store(count, Ordering::Relaxed);
}

/// Get page fault count
pub fn get_page_faults() -> usize {
    PAGE_FAULTS_TOTAL.load(Ordering::Relaxed)
}

/// Get demand allocation count
pub fn get_demand_allocations() -> usize {
    DEMAND_ALLOCATIONS.load(Ordering::Relaxed)
}

/// Get COW fault count
pub fn get_cow_faults() -> usize {
    COW_FAULTS.load(Ordering::Relaxed)
}

/// Get frames shared count
pub fn get_frames_shared() -> usize {
    FRAMES_SHARED.load(Ordering::Relaxed)
}

/// Print summary of all counters
pub fn print_summary() {
    println!("VM Statistics:");
    println!("  Page faults: {}", get_page_faults());
    println!("  Demand allocations: {}", get_demand_allocations());
    println!("  COW faults: {}", get_cow_faults());
    println!("  Frames shared: {}", get_frames_shared());
}

/// Reset all counters (for testing)
#[allow(dead_code)]
pub fn reset_counters() {
    PAGE_FAULTS_TOTAL.store(0, Ordering::Relaxed);
    DEMAND_ALLOCATIONS.store(0, Ordering::Relaxed);
    COW_FAULTS.store(0, Ordering::Relaxed);
    FRAMES_SHARED.store(0, Ordering::Relaxed);
}
