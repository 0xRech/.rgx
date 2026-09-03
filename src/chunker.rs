pub const MIN_CHUNK_SIZE: usize = 64 * 1024;
pub const TARGET_CHUNK_SIZE: usize = 256 * 1024;
pub const MAX_CHUNK_SIZE: usize = 1024 * 1024;

const WINDOW_SIZE: usize = 64;
const BOUNDARY_MASK: u64 = (TARGET_CHUNK_SIZE as u64) - 1;

#[derive(Debug, Clone)]
pub struct RollingHash {
    state: u64,
    window: [u8; WINDOW_SIZE],
    position: usize,
    filled: usize,
}

impl Default for RollingHash {
    fn default() -> Self {
        Self {
            state: 0,
            window: [0u8; WINDOW_SIZE],
            position: 0,
            filled: 0,
        }
    }
}

impl RollingHash {
    pub fn push(&mut self, byte: u8) -> u64 {
        if self.filled < WINDOW_SIZE {
            self.state = self.state.rotate_left(1) ^ gear_value(byte);
            self.window[self.position] = byte;
            self.position = (self.position + 1) % WINDOW_SIZE;
            self.filled += 1;
            return self.state;
        }

        let outgoing = self.window[self.position];
        self.window[self.position] = byte;
        self.position = (self.position + 1) % WINDOW_SIZE;
        self.state = self.state.rotate_left(1)
            ^ gear_value(byte)
            ^ gear_value(outgoing).rotate_left((WINDOW_SIZE % 64) as u32);
        self.state
    }
}

pub fn should_cut(state: u64, len: usize) -> bool {
    len >= MAX_CHUNK_SIZE || (len >= MIN_CHUNK_SIZE && (state & BOUNDARY_MASK) == 0)
}

fn gear_value(byte: u8) -> u64 {
    // SplitMix64-derived deterministic value. This rolling hash only chooses
    // content-defined chunk boundaries; BLAKE3 remains the integrity primitive.
    let mut value = (byte as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_boundaries_never_exceed_maximum() {
        let mut rolling = RollingHash::default();
        let mut len = 0usize;
        let mut max_seen = 0usize;

        for i in 0..(8 * MAX_CHUNK_SIZE) {
            let byte = ((i * 131) & 0xff) as u8;
            let state = rolling.push(byte);
            len += 1;
            if should_cut(state, len) {
                max_seen = max_seen.max(len);
                assert!(len >= MIN_CHUNK_SIZE);
                assert!(len <= MAX_CHUNK_SIZE);
                len = 0;
            }
        }

        assert!(max_seen > 0);
        assert!(max_seen <= MAX_CHUNK_SIZE);
    }
}
