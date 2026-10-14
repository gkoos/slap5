//! Padded linear bitboards: cell (r, c) is bit r*STRIDE + c, column 15 is always empty.

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
}

#[derive(Clone, Copy)]
pub struct Board<B: Bits> {
    pub stones: [B; 2],
    valid: B,
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
        Board { stones: [B::ZERO; 2], valid }
    }

    #[inline(always)]
    pub fn make(&mut self, player: usize, r: usize, c: usize) {
        self.stones[player].set(r * STRIDE + c);
    }

    #[inline(always)]
    pub fn unmake(&mut self, player: usize, r: usize, c: usize) {
        self.stones[player].clear(r * STRIDE + c);
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
}
