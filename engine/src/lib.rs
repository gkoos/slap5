#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
mod avx;
pub mod bitboard;
pub mod clock;
pub mod eval;
pub mod game;
pub mod protocol;
pub mod search;
pub mod threats;
pub mod zobrist;

#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
pub type Words = avx::U256;
#[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
pub type Words = bitboard::U64x4;

pub type Board = bitboard::Board<Words>;

#[cfg(test)]
mod tests;
