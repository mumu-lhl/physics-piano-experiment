/// Deterministic xorshift32 noise source for real-time signal generation.
#[derive(Debug, Clone)]
pub struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    pub fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }

    #[inline(always)]
    pub fn next_f64(&mut self) -> f64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        (self.state as f64) / 2_147_483_648.0 - 1.0
    }
}

/// Deterministic xorshift64 noise source for real-time signal generation.
#[derive(Debug, Clone)]
pub struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    #[inline(always)]
    pub fn next_f64(&mut self) -> f64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state as f64 / u64::MAX as f64) * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::{XorShift32, XorShift64};

    #[test]
    fn noise_sources_are_bounded_and_non_constant() {
        let mut short = XorShift32::new(0x1337_BEEF);
        let mut long = XorShift64::new(0x8543_9281_4472_9103);
        let first_short = short.next_f64();
        let first_long = long.next_f64();

        assert!((-1.0..=1.0).contains(&first_short));
        assert!((-1.0..=1.0).contains(&first_long));
        assert_ne!(first_short, short.next_f64());
        assert_ne!(first_long, long.next_f64());
    }
}
