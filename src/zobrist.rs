//! Zobrist keys: one random 64-bit key per (player, bit index), generated at compile time.

const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

const fn build() -> [[u64; 256]; 2] {
    let mut state = 0x1234_5678_9ABC_DEF0_u64;
    let mut keys = [[0u64; 256]; 2];
    let mut p = 0;
    while p < 2 {
        let mut i = 0;
        while i < 256 {
            keys[p][i] = splitmix64(&mut state);
            i += 1;
        }
        p += 1;
    }
    keys
}

static KEYS: [[u64; 256]; 2] = build();

/// XORed into the key when player 1 is to move.
pub const SIDE: u64 = 0xD1B5_4A32_D192_ED03;

/// Key for a stone of `player` at bit index `idx` (row * STRIDE + col).
#[inline(always)]
pub fn key(player: usize, idx: usize) -> u64 {
    KEYS[player][idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_distinct_and_nonzero() {
        let mut all: Vec<u64> = (0..2).flat_map(|p| (0..256).map(move |i| key(p, i))).collect();
        assert!(all.iter().all(|&k| k != 0));
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), 512);
    }
}
