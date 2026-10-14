use crate::bitboard::Bits;
use core::arch::x86_64::*;

#[derive(Clone, Copy)]
pub struct U256(__m256i);

// A table avoids building the mask with scalar stores followed by a vector load (store-forwarding stall).
static BIT_MASKS: [[u64; 4]; 256] = {
    let mut t = [[0u64; 4]; 256];
    let mut i = 0;
    while i < 256 {
        t[i][i / 64] = 1 << (i % 64);
        i += 1;
    }
    t
};

impl U256 {
    #[inline(always)]
    fn words(self) -> [u64; 4] {
        // SAFETY: __m256i and [u64; 4] are both 32 bytes of plain data.
        unsafe { core::mem::transmute(self.0) }
    }
    #[inline(always)]
    fn from_words(w: [u64; 4]) -> Self {
        // SAFETY: see `words`.
        U256(unsafe { core::mem::transmute(w) })
    }
}

// SAFETY (whole module): compiled only when the avx2 target feature is enabled.
// Move whole 64-bit lanes toward lower indices, filling with zeros.
#[inline(always)]
fn lanes_down(x: __m256i, w: u32) -> __m256i {
    unsafe {
        let z = _mm256_setzero_si256();
        match w {
            0 => x,
            1 => _mm256_blend_epi32::<0b1100_0000>(_mm256_permute4x64_epi64::<0b00_11_10_01>(x), z),
            2 => _mm256_blend_epi32::<0b1111_0000>(_mm256_permute4x64_epi64::<0b01_00_11_10>(x), z),
            3 => _mm256_blend_epi32::<0b1111_1100>(_mm256_permute4x64_epi64::<0b10_01_00_11>(x), z),
            _ => z,
        }
    }
}

// Move whole 64-bit lanes toward higher indices, filling with zeros.
#[inline(always)]
fn lanes_up(x: __m256i, w: u32) -> __m256i {
    unsafe {
        let z = _mm256_setzero_si256();
        match w {
            0 => x,
            1 => _mm256_blend_epi32::<0b0000_0011>(_mm256_permute4x64_epi64::<0b10_01_00_11>(x), z),
            2 => _mm256_blend_epi32::<0b0000_1111>(_mm256_permute4x64_epi64::<0b01_00_11_10>(x), z),
            3 => _mm256_blend_epi32::<0b0011_1111>(_mm256_permute4x64_epi64::<0b00_11_10_01>(x), z),
            _ => z,
        }
    }
}

impl Bits for U256 {
    const ZERO: Self = U256(unsafe { core::mem::transmute([0u64; 4]) });

    #[inline(always)]
    fn and(self, o: Self) -> Self {
        U256(unsafe { _mm256_and_si256(self.0, o.0) })
    }
    #[inline(always)]
    fn or(self, o: Self) -> Self {
        U256(unsafe { _mm256_or_si256(self.0, o.0) })
    }
    #[inline(always)]
    fn not(self) -> Self {
        U256(unsafe { _mm256_xor_si256(self.0, _mm256_set1_epi64x(-1)) })
    }
    #[inline(always)]
    fn shl(self, n: u32) -> Self {
        let (w, b) = (n / 64, n % 64);
        let x = lanes_up(self.0, w);
        if b == 0 {
            return U256(x);
        }
        unsafe {
            let lo = _mm256_sll_epi64(x, _mm_cvtsi32_si128(b as i32));
            let carry = _mm256_srl_epi64(lanes_up(x, 1), _mm_cvtsi32_si128(64 - b as i32));
            U256(_mm256_or_si256(lo, carry))
        }
    }
    #[inline(always)]
    fn shr(self, n: u32) -> Self {
        let (w, b) = (n / 64, n % 64);
        let x = lanes_down(self.0, w);
        if b == 0 {
            return U256(x);
        }
        unsafe {
            let lo = _mm256_srl_epi64(x, _mm_cvtsi32_si128(b as i32));
            let carry = _mm256_sll_epi64(lanes_down(x, 1), _mm_cvtsi32_si128(64 - b as i32));
            U256(_mm256_or_si256(lo, carry))
        }
    }
    #[inline(always)]
    fn is_zero(self) -> bool {
        unsafe { _mm256_testz_si256(self.0, self.0) != 0 }
    }
    #[inline(always)]
    fn count_ones(self) -> u32 {
        self.words().iter().map(|w| w.count_ones()).sum()
    }
    #[inline(always)]
    fn set(&mut self, i: usize) {
        *self = self.or(Self::from_words(BIT_MASKS[i]));
    }
    #[inline(always)]
    fn clear(&mut self, i: usize) {
        *self = self.and(Self::from_words(BIT_MASKS[i]).not());
    }
    #[inline(always)]
    fn get(self, i: usize) -> bool {
        (self.words()[i / 64] >> (i % 64)) & 1 != 0
    }
    #[inline(always)]
    fn pop_lowest(&mut self) -> Option<usize> {
        let mut w = self.words();
        for i in 0..4 {
            if w[i] != 0 {
                let bit = w[i].trailing_zeros() as usize;
                w[i] &= w[i] - 1;
                *self = Self::from_words(w);
                return Some(i * 64 + bit);
            }
        }
        None
    }
}
