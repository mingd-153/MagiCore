/// Retry strategies for HTTP requests
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum RetryStrategy {
    /// Fixed delay between retries
    Fixed(Duration),
    /// Exponential backoff
    Exponential { base: Duration, max: Duration },
}

impl RetryStrategy {
    pub fn delay(&self, attempt: u32) -> Duration {
        match self {
            Self::Fixed(d) => *d,
            Self::Exponential { base, max } => {
                // Saturating math (CI fix 2026-09-11): with large attempt
                // counts 2^attempt overflows f64 to +inf → Duration::mul_f64
                // PANICS ("cannot convert float seconds"). Cap the exponent
                // at 62 (beyond any real backoff window) and min() with
                // the configured max — a dead endpoint must produce a
                // bounded retry delay, never a process panic.
                // Toán bão hòa (fix CI): attempt lớn khiến 2^attempt tràn
                // f64 ra +inf → Duration::mul_f64 PANIC ("cannot convert
                // float seconds"). Kẹp mũ tại 62 (vượt mọi cửa sổ backoff
                // thực) rồi min() với max cấu hình — endpoint chết chỉ sinh
                // delay retry hữu hạn, không bao giờ panic cả process.
                let exp = attempt.min(62) as i32;
                let factor = 2_f64.powi(exp);
                let delay = base.mul_f64(factor);
                delay.min(*max)
            }
        }
    }
}
