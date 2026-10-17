//! The browser face of the engine: the page's occupancy words in, moves and per-iteration reports out.
//!
//! The page this serves used to run a JavaScript engine behind a Web Worker, and its message contract is
//! unchanged - one request, then a report per completed iteration, then the move. What changed is the
//! thing behind it: the search the Piskvork binary runs, compiled to wasm32, so the iteration reports
//! follow the engine's own rules (an interrupted iteration is discarded, a mate is verified) instead of a
//! second implementation's.
//!
//! `js_sys::Function` and `JsError` are the parts of this that cannot be exercised natively, so the
//! marshalling and the search call are plain functions with a Rust callback, and the exported methods are
//! thin wrappers over them: `cargo test -p slap5-wasm` covers everything except the JS calls themselves.

use slap5::Board;
use slap5::bitboard::{Bits, SIZE, STRIDE};
use slap5::search::{Limits, SearchResult, Searcher};
use std::time::Duration;
use wasm_bindgen::prelude::*;

/// Occupancy words the page sends: eight `u32`s covering the 225 cells, bit `row * 15 + col`, so only
/// bit 0 of the last word belongs to the board. That is the packing the page's own utilities use.
const WORDS: usize = 8;
const CELLS: usize = SIZE * SIZE;
/// 2^19 entries of 16 bytes = 8 MB. The native default is 2^21 (32 MB), which is affordable in a
/// long-lived process and not in a page where every cancelled search starts a new worker.
const DEFAULT_TT_BITS: u32 = 19;

/// A position as the page holds it, in the engine's own representation.
struct Position {
    board: Board,
    side: usize,
}

/// Decodes the two occupancy word arrays. Rejects what the engine cannot represent: arrays that are not
/// eight words, a padding bit set, and a cell holding both colours - a position the page's own rules call
/// invalid, but which would otherwise be searched as though the stones were one colour.
fn decode(black: &[u32], white: &[u32], black_to_move: bool) -> Result<Position, String> {
    if black.len() != WORDS || white.len() != WORDS {
        return Err(format!(
            "expected {WORDS} occupancy words per colour, got {} and {}",
            black.len(),
            white.len()
        ));
    }
    let mut board = Board::new();
    for (player, words) in [black, white].into_iter().enumerate() {
        for (word, &bits) in words.iter().enumerate() {
            let mut rest = bits;
            while rest != 0 {
                let bit = rest.trailing_zeros() as usize;
                rest &= rest - 1;
                let cell = word * 32 + bit;
                if cell >= CELLS {
                    return Err(format!("padding bit {cell} set in occupancy word {word}"));
                }
                let (r, c) = (cell / SIZE, cell % SIZE);
                let index = r * STRIDE + c;
                if board.stones[0].get(index) || board.stones[1].get(index) {
                    return Err(format!("both colours on cell x={c}, y={r}"));
                }
                board.make(player, r, c);
            }
        }
    }
    Ok(Position { board, side: usize::from(!black_to_move) })
}

/// The search, with the page's callback kept out of it. `report` is `Searcher::search_observing`'s: one
/// call per completed iteration, as (depth, x, y, score).
fn run(
    searcher: &mut Searcher,
    position: &Position,
    time_ms: f64,
    max_depth: i32,
    report: &mut dyn FnMut(i32, usize, usize, i32),
) -> Option<SearchResult> {
    let limits = Limits {
        time: Duration::from_secs_f64(time_ms.max(0.0) / 1000.0),
        max_depth: max_depth.max(1),
    };
    searcher.search_observing(&position.board, position.side, limits, report)
}

#[wasm_bindgen]
pub struct WasmEngine {
    searcher: Searcher,
    position: Position,
}

#[wasm_bindgen]
impl WasmEngine {
    /// One engine per worker. The transposition table, the killer moves and the history scores live as
    /// long as the worker does, so they carry over between the moves of a game the way they do natively.
    #[wasm_bindgen(constructor)]
    pub fn new(tt_bits: Option<u32>) -> WasmEngine {
        WasmEngine {
            searcher: Searcher::with_table_bits(tt_bits.unwrap_or(DEFAULT_TT_BITS).clamp(8, 24)),
            position: Position { board: Board::new(), side: 0 },
        }
    }

    /// `black` and `white` are the page's occupancy words (see `decode`); `black_to_move` is the side the
    /// search plays for.
    pub fn set_position(&mut self, black: &[u32], white: &[u32], black_to_move: bool) -> Result<(), JsError> {
        self.position = decode(black, white, black_to_move).map_err(|e| JsError::new(&e))?;
        Ok(())
    }

    /// Searches for `time_ms` or `max_depth`, whichever comes first, calling
    /// `on_iteration(depth, row, col, score)` after each completed iteration, in the page's coordinates.
    /// Returns null when there is nothing to play (a full board).
    pub fn search(&mut self, time_ms: f64, max_depth: i32, on_iteration: &js_sys::Function) -> Option<MoveResult> {
        let mut report = |depth: i32, x: usize, y: usize, score: i32| {
            // A callback that throws must not abandon the search: the iteration loop belongs to the
            // engine, and the page reports its own errors.
            on_iteration
                .call4(
                    &JsValue::NULL,
                    &JsValue::from(depth),
                    &JsValue::from(x as u32),
                    &JsValue::from(y as u32),
                    &JsValue::from(score),
                )
                .ok();
        };
        run(&mut self.searcher, &self.position, time_ms, max_depth, &mut report).map(MoveResult::from)
    }

    /// Applies one of the engine's switches (`Config::apply`), so the ablations the Rust side measures can
    /// be driven from the page too. False if the name is not one of them.
    pub fn set_config_flag(&mut self, flag: &str) -> bool {
        self.searcher.config.apply(flag)
    }
}

/// What the page needs from a finished search. The engine's `x` is a column and its `y` a row, which is
/// the protocol's (x, y) order; the page's board is indexed (row, col), so these getters swap them.
#[wasm_bindgen]
pub struct MoveResult {
    x: usize,
    y: usize,
    score: i32,
    depth: i32,
    mate_verified: Option<bool>,
    nodes: u64,
    elapsed_ms: f64,
}

impl From<SearchResult> for MoveResult {
    fn from(r: SearchResult) -> Self {
        MoveResult {
            x: r.x,
            y: r.y,
            score: r.score,
            depth: r.depth,
            mate_verified: r.mate_verified,
            nodes: r.stats.nodes,
            elapsed_ms: r.elapsed.as_secs_f64() * 1000.0,
        }
    }
}

#[wasm_bindgen]
impl MoveResult {
    #[wasm_bindgen(getter)]
    pub fn row(&self) -> usize {
        self.y
    }

    #[wasm_bindgen(getter)]
    pub fn col(&self) -> usize {
        self.x
    }

    /// Depth of the last completed iteration.
    #[wasm_bindgen(getter)]
    pub fn depth(&self) -> i32 {
        self.depth
    }

    #[wasm_bindgen(getter)]
    pub fn score(&self) -> i32 {
        self.score
    }

    /// `null` when no mate was reported, `false` when one was reported but was too far away to verify.
    #[wasm_bindgen(getter)]
    pub fn mate_verified(&self) -> Option<bool> {
        self.mate_verified
    }

    #[wasm_bindgen(getter)]
    pub fn nodes(&self) -> u64 {
        self.nodes
    }

    #[wasm_bindgen(getter)]
    pub fn elapsed_ms(&self) -> f64 {
        self.elapsed_ms
    }
}

#[cfg(test)]
mod tests;

