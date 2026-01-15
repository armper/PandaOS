//! Kernel invariant checking macros
//!
//! These macros provide aggressive runtime checks in debug builds
//! that are compiled away in release builds. Use them liberally to
//! catch corruption and logic errors early.

/// Check an invariant condition in debug builds only
///
/// In debug builds, this panics with the provided message if the condition is false.
/// In release builds, this compiles to nothing.
///
/// # Examples
///
/// ```ignore
/// kernel_invariant!(allocator.next_frame < allocator.end_frame,
///     "Frame allocator: next_frame {} >= end_frame {}",
///     allocator.next_frame, allocator.end_frame
/// );
/// ```
#[macro_export]
macro_rules! kernel_invariant {
    ($cond:expr, $($arg:tt)+) => {
        #[cfg(debug_assertions)]
        {
            if !($cond) {
                panic!("INVARIANT VIOLATION: {}", format_args!($($arg)+));
            }
        }
    };
}

/// Check that a value is within a valid range in debug builds
///
/// # Examples
///
/// ```ignore
/// kernel_invariant_range!(frame_num, 0, max_frames,
///     "Frame number out of range"
/// );
/// ```
#[macro_export]
macro_rules! kernel_invariant_range {
    ($val:expr, $min:expr, $max:expr, $($arg:tt)+) => {
        $crate::kernel_invariant!(
            ($val) >= ($min) && ($val) < ($max),
            "{}: {} not in range [{}..{})",
            format_args!($($arg)+),
            $val,
            $min,
            $max
        );
    };
}

/// Check that a pointer is non-null and properly aligned in debug builds
///
/// # Examples
///
/// ```ignore
/// kernel_invariant_ptr!(buffer_ptr, "VGA buffer pointer");
/// ```
#[macro_export]
macro_rules! kernel_invariant_ptr {
    ($ptr:expr, $name:expr) => {
        #[cfg(debug_assertions)]
        {
            let ptr = $ptr as *const u8;
            $crate::kernel_invariant!(!ptr.is_null(), "{} is null", $name);
            $crate::kernel_invariant!(
                ptr as usize % core::mem::align_of::<*const u8>() == 0,
                "{} is misaligned: {:p}",
                $name,
                ptr
            );
        }
    };
}

/// Check that a mutex/lock is not held in debug builds
///
/// Useful for detecting deadlock-prone code paths.
///
/// # Examples
///
/// ```ignore
/// kernel_invariant_not_locked!(ALLOCATOR_LOCK, "allocator");
/// ```
#[macro_export]
macro_rules! kernel_invariant_not_locked {
    ($lock:expr, $name:expr) => {
        #[cfg(debug_assertions)]
        {
            $crate::kernel_invariant!(
                $lock.try_lock().is_some(),
                "{} lock is already held (potential deadlock)",
                $name
            );
        }
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_kernel_invariant_passes() {
        kernel_invariant!(true, "This should not panic");
        kernel_invariant!(1 + 1 == 2, "Math works");
    }

    #[test]
    #[should_panic(expected = "INVARIANT VIOLATION")]
    #[cfg(debug_assertions)]
    fn test_kernel_invariant_fails_in_debug() {
        kernel_invariant!(false, "This should panic in debug");
    }

    #[test]
    fn test_kernel_invariant_range_passes() {
        kernel_invariant_range!(5, 0, 10, "Value in range");
    }

    #[test]
    #[should_panic(expected = "INVARIANT VIOLATION")]
    #[cfg(debug_assertions)]
    fn test_kernel_invariant_range_fails() {
        kernel_invariant_range!(15, 0, 10, "Value out of range");
    }

    #[test]
    fn test_kernel_invariant_ptr_passes() {
        let x = 42u64;
        kernel_invariant_ptr!(&x, "test pointer");
    }

    #[test]
    #[should_panic(expected = "INVARIANT VIOLATION")]
    #[cfg(debug_assertions)]
    fn test_kernel_invariant_ptr_fails_null() {
        kernel_invariant_ptr!(core::ptr::null::<u64>(), "null pointer");
    }
}
