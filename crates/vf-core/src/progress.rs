//! Speed and time left of a transfer (a download, an install), from what is done and the time spent.
//! Whole numbers only: bytes, bytes per second, whole seconds.

/// Average speed in units per second: `done` over `elapsed_ms`. `0` until a millisecond has passed.
pub fn speed(done: u64, elapsed_ms: u64) -> u64 {
    if elapsed_ms == 0 {
        return 0;
    }
    let per_second = u128::from(done).saturating_mul(1000) / u128::from(elapsed_ms);
    u64::try_from(per_second).unwrap_or(u64::MAX)
}

/// Seconds left at the average speed so far, rounded up; `0` once `done` reaches `total`. `None`
/// while nothing is done yet (no speed to go by).
pub fn eta(done: u64, total: u64, elapsed_ms: u64) -> Option<u64> {
    let left = total.saturating_sub(done);
    if left == 0 {
        return Some(0);
    }
    if done == 0 || elapsed_ms == 0 {
        return None;
    }
    // left / (done / elapsed) = left * elapsed / done, in milliseconds.
    let ms = u128::from(left).saturating_mul(u128::from(elapsed_ms)) / u128::from(done);
    Some(u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_is_the_average_so_far() {
        assert_eq!(speed(0, 0), 0);
        assert_eq!(speed(10_000, 0), 0);
        assert_eq!(speed(10_000, 2_000), 5_000);
        assert_eq!(speed(1, 3), 333);
        assert_eq!(speed(u64::MAX, 1), u64::MAX);
    }

    #[test]
    fn eta_goes_by_the_average_speed() {
        // 12 of 38 MB in 4 s: 3 MB/s, 26 MB left — 9 s (8.67 rounded up).
        assert_eq!(eta(12_000_000, 38_000_000, 4_000), Some(9));
        assert_eq!(eta(0, 38_000_000, 4_000), None);
        assert_eq!(eta(5, 10, 0), None);
        assert_eq!(eta(10, 10, 4_000), Some(0));
        assert_eq!(eta(12, 10, 4_000), Some(0));
        assert_eq!(eta(1, u64::MAX, u64::MAX), Some(u64::MAX));
    }
}
