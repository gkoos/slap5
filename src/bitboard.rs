//! Padded linear bitboards: cell (r, c) is bit r*STRIDE + c, column 15 is always empty.

use crate::zobrist;

pub const SIZE: usize = 15;
pub const STRIDE: usize = 16;
pub const DIRS: [u32; 4] = [1, 16, 15, 17];

pub trait Bits: Copy {
    const ZERO: Self;
    fn and(self, o: Self) -> Self;
    fn or(self, o: Self) -> Self;
    fn not(self) -> Self;
    fn shl(self, n: u32) -> Self;
    fn shr(self, n: u32) -> Self;
    fn is_zero(self) -> bool;
    fn count_ones(self) -> u32;
    fn set(&mut self, i: usize);
    fn clear(&mut self, i: usize);
    fn get(self, i: usize) -> bool;
    /// Clears the lowest set bit and returns its index.
    fn pop_lowest(&mut self) -> Option<usize>;
    /// The four backing words, lowest first. Both representations keep 256 bits in four `u64`s, so the
    /// read-only iterator below can be built without knowing which one it is.
    fn words(self) -> [u64; 4];
    /// Iterates the set bits in ascending cell order, extracting the backing words once. Use this when the
    /// mask is only read: `pop_lowest` clears one bit but rebuilds the whole vector to do it.
    #[inline(always)]
    fn bits(self) -> BitsIter {
        BitsIter { words: self.words(), word: 0 }
    }
}

/// The set bits of a mask in ascending cell order, with the backing words extracted once (see `Bits::bits`).
pub struct BitsIter {
    words: [u64; 4],
    word: usize,
}

impl Iterator for BitsIter {
    type Item = usize;

    #[inline(always)]
    fn next(&mut self) -> Option<usize> {
        while self.word < 4 {
            let w = self.words[self.word];
            if w != 0 {
                self.words[self.word] = w & (w - 1);
                return Some(self.word * 64 + w.trailing_zeros() as usize);
            }
            self.word += 1;
        }
        None
    }
}

#[derive(Clone, Copy)]
pub struct U64x4([u64; 4]);

impl Bits for U64x4 {
    const ZERO: Self = U64x4([0; 4]);

    #[inline(always)]
    fn and(self, o: Self) -> Self {
        let mut r = self.0;
        for i in 0..4 {
            r[i] &= o.0[i];
        }
        U64x4(r)
    }
    #[inline(always)]
    fn or(self, o: Self) -> Self {
        let mut r = self.0;
        for i in 0..4 {
            r[i] |= o.0[i];
        }
        U64x4(r)
    }
    #[inline(always)]
    fn not(self) -> Self {
        let mut r = self.0;
        for i in 0..4 {
            r[i] = !r[i];
        }
        U64x4(r)
    }
    #[inline(always)]
    fn shl(self, n: u32) -> Self {
        let (w, b) = ((n / 64) as usize, n % 64);
        let mut r = [0u64; 4];
        for i in w..4 {
            let lo = self.0[i - w];
            r[i] = if b == 0 {
                lo
            } else {
                let carry = if i > w { self.0[i - w - 1] >> (64 - b) } else { 0 };
                (lo << b) | carry
            };
        }
        U64x4(r)
    }
    #[inline(always)]
    fn shr(self, n: u32) -> Self {
        let (w, b) = ((n / 64) as usize, n % 64);
        let mut r = [0u64; 4];
        for i in 0..(4 - w) {
            let lo = self.0[i + w];
            r[i] = if b == 0 {
                lo
            } else {
                let carry = if i + w + 1 < 4 { self.0[i + w + 1] << (64 - b) } else { 0 };
                (lo >> b) | carry
            };
        }
        U64x4(r)
    }
    #[inline(always)]
    fn is_zero(self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }
    #[inline(always)]
    fn count_ones(self) -> u32 {
        self.0.iter().map(|w| w.count_ones()).sum()
    }
    #[inline(always)]
    fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }
    #[inline(always)]
    fn clear(&mut self, i: usize) {
        self.0[i / 64] &= !(1 << (i % 64));
    }
    #[inline(always)]
    fn get(self, i: usize) -> bool {
        (self.0[i / 64] >> (i % 64)) & 1 != 0
    }
    #[inline(always)]
    fn pop_lowest(&mut self) -> Option<usize> {
        for i in 0..4 {
            let w = self.0[i];
            if w != 0 {
                self.0[i] = w & (w - 1);
                return Some(i * 64 + w.trailing_zeros() as usize);
            }
        }
        None
    }
    #[inline(always)]
    fn words(self) -> [u64; 4] {
        self.0
    }
}

#[derive(Clone, Copy)]
pub struct Board<B: Bits> {
    pub stones: [B; 2],
    valid: B,
    hash: u64,
}

impl<B: Bits> Default for Board<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Bits> Board<B> {
    pub fn new() -> Self {
        let mut valid = B::ZERO;
        for r in 0..SIZE {
            for c in 0..SIZE {
                valid.set(r * STRIDE + c);
            }
        }
        Board { stones: [B::ZERO; 2], valid, hash: 0 }
    }

    /// Zobrist hash of the stones; the side to move is not included.
    #[inline(always)]
    pub fn hash(&self) -> u64 {
        self.hash
    }

    #[inline(always)]
    pub fn make(&mut self, player: usize, r: usize, c: usize) {
        let i = r * STRIDE + c;
        debug_assert!(!self.stones[0].get(i) && !self.stones[1].get(i), "cell occupied");
        self.stones[player].set(i);
        self.hash ^= zobrist::key(player, i);
    }

    #[inline(always)]
    pub fn unmake(&mut self, player: usize, r: usize, c: usize) {
        let i = r * STRIDE + c;
        debug_assert!(self.stones[player].get(i), "no such stone");
        self.stones[player].clear(i);
        self.hash ^= zobrist::key(player, i);
    }

    /// True if `player` has five or more in a row.
    #[inline]
    pub fn wins(&self, player: usize) -> bool {
        let x = self.stones[player];
        for d in DIRS {
            let r = x
                .and(x.shr(d))
                .and(x.shr(2 * d))
                .and(x.shr(3 * d))
                .and(x.shr(4 * d));
            if !r.is_zero() {
                return true;
            }
        }
        false
    }

    /// On-board cells holding no stone.
    #[inline(always)]
    pub fn empty(&self) -> B {
        self.stones[0].or(self.stones[1]).not().and(self.valid)
    }

    pub fn stone_count(&self) -> u32 {
        self.stones[0].count_ones() + self.stones[1].count_ones()
    }

    /// Empty cells within distance 2 (any direction) of a stone.
    #[inline]
    pub fn candidates(&self) -> B {
        // Masking after each step stops the guard column from wrapping a stone onto the next row.
        let grow = |x: B| {
            x.or(x.shl(1))
                .or(x.shr(1))
                .or(x.shl(15))
                .or(x.shr(15))
                .or(x.shl(16))
                .or(x.shr(16))
                .or(x.shl(17))
                .or(x.shr(17))
                .and(self.valid)
        };
        grow(grow(self.stones[0].or(self.stones[1]))).and(self.empty())
    }
}
