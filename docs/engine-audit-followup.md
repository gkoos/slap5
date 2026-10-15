# Follow-up engine audit — 15 October 2026

**The strongest newly demonstrated gap is runaway selective extension, coupled with the absence of an attacking tactical search at the horizon.** The revised engine recognizes richer patterns, but can spend a second following free open-three responses out to ply 64 without completing iteration two. PentaZen proves a win in the same position in about 1–2 ms. Charging depth for those open-three replies, while keeping exactly the same reply candidates, makes high5 report the win in **463 nodes**.

This is a narrower and better supported diagnosis than the first audit's broad evaluation/ordering explanation. It is based on the current source, current-binary replays, an independent continuous-four prover, and isolated experiments on a disposable source copy. No production source or default setting was changed by this audit.

## Current code and evidence

The prior recommendations have led to real implementation changes: per-direction classes are used for evaluation and ordering, weaker development classes now score, four blocks are free, the separate leaf search is gone, double threats receive extensions, and mate verification has been added. The old statement that all forks collapse to one ordering bit no longer describes the current implementation.

The examined release executable has SHA-256 prefix **7E3119E2AB3C095A**, matching the current arm of the fresh Tito match `logs/pbrain-Tito2014_20261015_140104`. Its result is **3W–16L–1 truncated**, against the old control's **5W–15L** in `..._140919`, on openings 25–34 at one second per turn. The corresponding fresh pela arms are 4W–15L–1 truncated and 5W–15L. All **58 production tests pass** and `cargo check --examples` passes.

The new test positions were taken immediately after high5 moves in the current Tito losses. The opponent is then to move. This tests the attacking half of the search, which the old last-chance suite samples poorly. A separate Python continuous-four prover found forcing certificates in four positions; a second implementation using a plain grid checked that every attacking move either wins or threatens five, a defender must block the unique winning cell or faces multiple winning cells, no defender can win instead of blocking, and the certificate ends in an actual five. These are positive win certificates. Failure to find a certificate is never labelled safe.

## 1. The free open-three extension can consume the entire turn on one root move

Relevant current code is `src/search.rs:549–595`:

- `three_reply` detects an opponent open-three threat.
- Every candidate reply receives the same depth counter while the search ply is below 64, **including counter-fours**.
- This applies even at remaining depth zero; `!free` is required before static evaluation is allowed.
- `forced` also disables both move-count pruning and late move reductions.
- The reply set is a union of a superset of blocking cells and all own four-making cells. It may branch and includes replies that fail to stop all the threats.

A four block is generally a single forced move. An open-three response has alternatives, and its counter-fours can force another free block before the original three still needs answering. Giving the entire branching exchange free depth allows very large trees at a nominal iteration depth of two. The root-ply cap eventually stops them, but 64 is far beyond a practical one-second tactical budget for this tree.

The worst sampled case is **opening 33, high5 second**, after decision index 20, with 46 stones on the board:

| Observation | Current default | Same candidates, open-three replies cost one ply |
|---|---:|---:|
| Completed depth | 1 | 2 |
| Returned score | +2586, ordinary evaluation | +29993, mate in 7 |
| Counted nodes | 778240 | 463 |
| Maximum visited ply | 64 | 6 |
| Open-three response nodes | 184831 | 144 |
| Such nodes reached at depth zero or below | 120199 | 129 |
| Root moves entered in iteration two | **1** | Iteration completes |
| Thinking time | 1000 ms | Under 1 ms in the instrumented run |

The default's depth-one hint is `(10,11)`. Iteration two explores that move first, gets stuck in the extended subtree, and never enters a second root move. Its completed result therefore remains the depth-one choice. This is a demonstrated failure to distribute the turn's work, not simply a low reported depth caused by extensions.

The isolated change was applied only to a copy under `logs/audit-followup-search`:

```rust
// Current:
let free = (three_reply || four_reply) && ply < self.config.free_ply_limit;
// Diagnostic: retain three_reply and its candidate restriction, charge its depth.
let free = four_reply && ply < self.config.free_ply_limit;
```

It retains the evaluator, ordering, TT, reductions, double-threat extension and open-three candidate set. The production switch `no-threes` changes both candidate restriction and extension; this experiment deliberately separates them.

## 2. The improvement repeats across the four certified winning positions

All queries use fresh searchers. Times are representative single runs, and boundary cases vary with timing. “Mate” below describes the engine's report; the independent certificates establish that these positions are winning, without asserting that every engine's chosen line is shortest.

| Attacking position | Default high5, 1000 ms search budget | Isolated depth-cost change | Supplied PentaZen, 1000 ms turn limit | Supplied Rapfi, same turn limit |
|---|---|---|---|---|
| Opening 28, high5 first, 53 stones | Mate in 11; depth 4; ~112 ms | Mate in 11; ~10 ms | `+v11`; ~1–2 ms search | `+M11` |
| Opening 28, high5 second, 46 stones | Ordinary score +2466; depth 4; no mate recognized | Mate in 9; depth 5; ~92 ms | `+v17`; ~2 ms search | `+M9` |
| Opening 30, high5 second, 40 stones | Ordinary score; depth 3–4; no mate recognized | Mate in 13; depth 7; ~925 ms | `+v19`; ~2 ms search | `+M13` |
| Opening 33, high5 second, 46 stones | Ordinary score +2586; depth 1; no mate recognized | Mate in 7; depth 2; 463 nodes | `+v6`; ~1–2 ms search | `+M7`; ~30 ms search |

Thus the default recognizes **1/4** wins in this sample; the isolated change recognizes **4/4**. These are deliberately selected tactical cases from losses, not a neutral benchmark or evidence of an Elo gain. They do demonstrate that the depth policy can hide short wins that the existing engine can otherwise find.

Additional controls support the diagnosis:

- `no-double` still leaves the worst case stuck at depth one. The double-threat extension is not sufficient to explain that failure.
- `no-limit` also leaves it stuck at depth one. Simply searching more moves does not fix the extension tree.
- Limiting free plies to root ply 12 makes it report mate in seven, but still takes roughly 147000 nodes. A smaller cap treats the symptom without recovering the 463-node search.
- Allowing free open-three responses only at positive remaining depth still leaves it stuck. A long free exchange can start before depth reaches zero.
- The simpler production switches `no-threes` and `no-fours` also remove the worst stall, showing the interaction of the two free-reply rules. The isolated experiment identifies the open-three depth cost without changing its move set.

The preceding defensive positions improve in useful ways too. In the opening-28-first case, the default repeats `(2,6)`, after which the independent prover finds an 11-ply continuous-four win in nine recursive calls. The isolated version chooses `(11,7)`; no continuous-four win is found after it within the prover's ten-attacking-move bound. That is not a safety proof. PentaZen and Rapfi choose other defensive moves and return ordinary evaluations. In the other three preceding positions the isolated version reports losses that the default misses, including the opening-33 defence where it completes depth five in about 2 ms instead of remaining at depth one after a second. Those loss reports are not independent exhaustive proofs of all possible defences.

## 3. Main-search extensions do not replace the reference engines' VCF search

The current implementation notes mark the continuous-four search as implemented through free defensive replies. That claim overstates equivalence.

At `src/search.rs:555–557`, when depth is exhausted and the side is not a free-reply node, high5 immediately returns static evaluation. **It does not try that side's voluntary fours.** Making the opponent's subsequent block free helps only if the attacking move was searched before the horizon. A long forcing continuation still competes with ordinary moves for main-search depth and ordering.

PentaZen explicitly calls VCF at depth zero (`external/pentazen/src/src/search.cpp:367–375`). Its VCF uses specialized attacking moves, localized continuation generation, and at most two moves below the VCF root (`search.cpp:620`, `movegen.cpp:201–246`). Rapfi explicitly transfers to `vcfsearch`/`vcfdefend` at depth zero (`external/rapfi/src/Rapfi/search/ab/search.cpp:1015–1019`), with localized continuation candidates and combination filters (`search/movepick.cpp:213–220,445–455`). Both preserve a distinct tactical search policy even though ordinary search also extends forced four blocks.

PentaZen's 1–2 ms wins are especially useful evidence: its classical evaluator and specialized forcing search already demonstrate a large gap on these cases. The immediate implementation target is an efficient tactical frontier, alongside a depth policy that prevents branching open-three replies from consuming the turn. Adding another broad extension is unlikely to address this measured failure.

## 4. Mate verification largely verifies its own cached result

`verify_mate` turns off `selective` at `src/search.rs:421`, but leaves TT cutoffs enabled and reuses the table filled by the selective search. Its children can return those earlier approximate scores without doing the broader work verification is intended to perform.

In the opening-28-first replay, the initial search uses roughly 49000 nodes. Its purported verification visits **120 nodes, of which 119 return through TT cutoffs**. In the isolated winning replays, verification similarly takes 124 nodes/123 TT cutoffs, 112/111 and 16/15. These are directly instrumented control-flow counts.

Disabling TT only during verification makes the first replay spend about **766000 additional nodes**, hit the one-second deadline, and still return the original mate report. The timeout behaviour is explicit: `verify_mate` returns `None` on interruption, the caller retains the unverified result, and the iteration loop breaks (`src/search.rs:398–406,425–429`). The documentation statement “a mate is verified before it decides a move” is therefore not guaranteed.

This audit did **not** demonstrate a naturally false mate in a saved game. The finding is a correctness and validation gap: the verification mechanism is not independent, and an interrupted verification does not mark a result inconclusive. A sound tactical certificate, trusted-bound tags, or a genuinely independent bounded verification path is needed before treating these reports as proofs. Preserve a legal fallback move if verification cannot finish.

## 5. The richer evaluation remains linear, and expensive work still precedes TT probes

The per-direction change is useful, but it is not the complete PentaZen/Rapfi position model. `src/eval.rs:12–31` still sums six untuned counts of hypothetical shape-making cells. Cross-direction threats now count twice; there is no nonlinear combination value or dynamic initiative term. PentaZen evaluates existing material and averages current/previous states. Rapfi additionally uses combined pattern classes and a side-to-move tactical table (`eval/eval.cpp:33–116`). These remain plausible strategic gaps, but the new controlled evidence prioritizes search allocation first.

Every call to `node` also computes both players' full `Shapes` before an immediate tactical decision, horizon evaluation or TT cutoff (`src/search.rs:539`). `Shapes` builds all six classes in all four directions for both players. Even `five_cells` calls the general `dir_shapes` routine before selecting one class (`src/threats.rs:171–183`); it has no dedicated cheap five-only implementation. LLVM may eliminate unused class calculations after inlining, so the latter is a profiling question rather than a measured cost claim.

Both reference engines incrementally maintain the relevant patterns, and their forcing move generators use smaller specialized sets. Reordering cheap tactical checks, TT probing and full scoring deserves profiling once the extension problem is controlled. No wall-time attribution to individual mask calculations was made here.

## 6. Flat aggregate wins conceal large changes in individual outcomes

The fresh Tito comparison has **no shared wins**: the new engine wins three games the control loses, while the control wins five games the new engine loses. Eleven are shared losses, and one new-engine game is truncated where the control loses. Equal or similar total win counts do not mean the code changed no won game.

Also, the earlier 7–13 block and the fresh 3–16–1 block have different recorded executable hashes. **One correction, from re-reading the run records:** the 7–13 figure was produced by the runs at 12:34/12:42 (`logs/pbrain-Tito2014_20261015_123410` and `..._124202`, openings 10–19, 3W7L and 4W6L) whose recorded hash is **`48922BBBF9C90D71`**; **`5DDB4164D440C886`** is a third build, used by the runs at 11:54/11:57 on the same openings, which scored **3W–17L** (1W9L and 2W8L). The substance of the point is unaffected — `48922BBBF9C90D71` and `7E3119E2AB3C095A` are different builds, so the two blocks are not one executable — but the hash must be read from each run's `run-info.txt`, and the control's hash is recorded only for the fresh runs (`6B26D3EE04B1F1CA`), leaving the earlier block's control build unprovable. They should not be described as an exact replication of the same executable without accounting for the intervening changes. The run information is useful, but a Git revision plus a dirty-file count cannot reconstruct an uncommitted build. Archive the binary or a complete patch with each experiment.

The existing 20-unique-position saving suite rewards defensive extension policy. It misses the attacking iteration-two stalls demonstrated here. Keeping free branching extensions because that suite rises from 17/21 to 20/21 can therefore select the wrong tradeoff for whole games. Pair defensive cases with attacking wins, short tactical time budgets, and explicit iteration-completion measurements.

## Reading the losses (15 October 2026)

Five games on openings 25–34 that the pre-work control **won** and the current build **lost** were traced through the engine log. In each one the score is balanced, then one decision later it is a mate against us: the game is lost by a single move. For each game the last balanced decision was then asked the same question the corpus above asks of its certificates — after our move, does the board hand the opponent a forced continuous-four win?

| game | last balanced decision | our move there | our score / depth | forced four-win for them after it? |
|---|---|---|---|---|
| opening 26 first | 8 of 15 | (7,4) | −788 / 4 | no |
| opening 28 first | 23 of 29 | (6,3) | **+217** / 4 | **yes** |
| opening 30 first | 23 of 31 | (12,6) | −661 / 5 | no |
| opening 33 first | 5 of 13 | (8,8) | −391 / 4 | no |
| opening 33 second | 15 of 24 | (3,8) | −584 / 4 | no |

Two things follow, and neither is what the earlier steps assumed.
1. **Four of the five kill moves are not four-chains.** A 5000-node continuous-four prover finds nothing for the opponent after our move, yet we are mated a few plies later. So the defensive blind spot is overwhelmingly in *three*-based chains (and longer plans), not in the four search: another VCF at the horizon would not have saved any of these four. The next probe is to ask the two reference engines (they have VCT) what they see in those four positions, using `scripts/audit_engine.py`.
2. **The one four-chain case is a plain search miss, not a knowledge gap.** Opening 28 first, decision 23: we played (6,3) with a score of **+217** at depth 4 and 52 stones on the board, and the opponent then forced a win with continuous fours that the prover finds in a few thousand nodes. Our own forcing rules exist for exactly this and did not fire. That single position is the cheapest concrete bug available, and it should be checked before anything new is built: search it at 1 s, 4 s and 16 s and see whether the score ever turns negative.

### What that probe showed (same day)

The five positions were re-searched as our side to move at 1 s, 4 s and 16 s, and the prover was asked about the moves afterwards.

| game | our score in the match (depth) | 1 s | 4 s | 16 s |
|---|---|---|---|---|
| opening 26 first, decision 8 | −788 (4) | −631 (4) | −631 (4) | **−29982 (5), found in 4.4 s** |
| opening 28 first, decision 23 | **+217** (4) | +181 (4) | +2 (5) | +2 (5), depth 6 never completes |
| opening 30 first, decision 23 | −661 (5) | −894 (5) | **−29980 (6), 2.2 s** | −29980 (6) |
| opening 33 first, decision 5 | −391 (4) | −282 (4) | −282 (4) | −307 (6) |
| opening 33 second, decision 15 | −584 (4) | −568 (2) | −499 (3) | −1120 (5) |

Three of the five are one ply short. In two of them our own search finds the mate against us at depth 5–6 in 2–4 s, and at 1 s it sits at depth 4–5 and cannot see it. In the third (opening 28 first) the move we played, (6,3), allows a forced four-win in 11 plies, and so does the move the 1 s search prefers, (2,6); only (11,9) is safe, and the engine does find (11,9) at 4 s and 16 s — with the score honestly down from **+217 to +2**. So the move choice and the evaluation were both wrong only because depth 4 was reached instead of depth 5.

The remaining two are outside the horizon altogether: at 16 s the score is −307 and −1120 with no mate against us seen, and the prover finds no four-win either, so those are three-based or slower losses that no amount of this search reaches.

**The consequence is about efficiency, not features.** Time is not the lever: 4 s buys the same depth as 1 s in four of these five positions, and 16 s reaches depth 5–6 with 11.7 million nodes. What is missing is one more ply inside 1 s, so the work is to make a ply cheaper and the tree smaller — node cost, pruning, table use, the horizon evaluation — and the baseline to beat is now written down here: depth 4, 4, 5, 4, 2 at 1 s on these five positions.

1. Make open-three candidate restriction and its depth extension independent settings. Test a depth cost or tightly bounded extension for branching responses; in particular, account for counter-fours rather than giving every reply unlimited free depth. Keep the current and isolated versions available for comparison. **Done and measured (15 October 2026):** the two are now separate settings (`free_three_replies` and `extend_three_replies`, switch `no-three-ext`), the isolated version reproduces this audit's numbers exactly from the production binary (1/4 → 4/4 proved wins, opening 33 mate in 7 in 463 nodes), the defensive suite drops 20/21 → 16/21 with it, and **over 40 full games on openings 25–34 against Tito and pela it scored 8 wins against the default build's 7 and the pre-work control's 10** (Tito 3W–17L, pela 5W–14L–1T; 5 of the 20 Tito games differ between the two builds). The default is unchanged and the switch is off: the defect this audit demonstrated is real, and fixing it did not win a game. **The attacking corpus was also grown past the four positions**: the same extraction over four loss sets (Tito 10–19, Tito 25–34, pela 20–24, pela 25–34) certifies **7 positions in about 44 losses**, and the default proves **3** of the 7 while `no-three-ext` proves **6** — including one position (opening 18 second) that the default proves and the switch does not, so the two effects cut both ways and both are small. Missed short wins are rare in the recorded losses, which is the second reason not to build the horizon search next. `scripts/audit_followup.py` now finds the run's high5 log by pattern instead of the fixed name `pbrain-high5.exe.log`, which is what stopped it from reading the `no-three-ext` arms.
2. Restore a specialized, selective tactical search at the horizon: ranked continuous-four continuations, forced defender blocks, explicit node/time limits, and inconclusive results distinguished from proved wins. Use these four certified positions as regressions, alongside the defensive suite. The previous broad, board-wide leaf search is not the design to restore unchanged.
3. Make mate verification independent of selective cached bounds and expose whether it completed. Do not use the verification test as evidence of proof while the verifier can simply consume the original result.
4. Profile pattern recomputation and TT hit costs, then revisit positional/initiative evaluation. Those are further opportunities, not substitutes for fixing the demonstrated search stalls.
5. Judge any proposed default change on paired matches against both opponents. This audit deliberately did not infer a match-strength gain from four selected positions or modify production settings.

## Reproduction and artifacts

```powershell
cargo test
python -B scripts/audit_followup.py --nodes 1500 --replay --ms 1000
cargo run --release --example audit_followup -- 1000
python -B scripts/audit_search_modes.py --mode default --ms 1000
python -B scripts/audit_search_modes.py --mode no-free-three --ms 1000
python -B scripts/audit_search_modes.py --mode positive-three --ms 1000
python -B scripts/audit_search_modes.py --mode fresh-verify --ms 1000
# Add 'defend' to the last tool's arguments to inspect the preceding defensive positions.
cargo run --release --example audit_followup -- 1000 cap=12     # the free-ply cap control
cargo run --release --example audit_followup -- 1000 no-double  # these three are Config::apply switches
cargo run --release --example audit_followup -- 1000 no-limit
cargo run --release --example audit_followup -- 1000 no-threes
```

**Independently reproduced on 15 October 2026, from this commit's tree:** the default stall (opening 33 second, depth 1, +2586, `max_ply` 64, one root move entered in iteration two, node counts within 3 %), the isolated depth-cost column (mate in 11 at 10 ms, mate in 9 at depth 5, mate in 13 at depth 7, mate in 7 at depth 2 in **463 nodes** — every figure to the digit), the four controls (`no-double` and `no-limit` stuck at depth one, `cap=12` mate in seven at 146569 nodes, `positive-three` stuck), the verification counts (120 nodes, 119 table cutoffs; with the table off during verification 814080 nodes against 49318 and the deadline hit), the code readings at `src/search.rs:549–595`, `src/search.rs:555–557`, `src/search.rs:421/425–427`, `src/threats.rs:177–183` and `src/eval.rs:12–31`, the release hash `7E3119E2AB3C095A`, the 58 passing tests and `cargo check --examples`. The fresh-match outcome analysis also holds exactly: 3 new-only wins, 5 control-only wins, 11 shared losses, 1 truncated. Not re-run here: the PentaZen and Rapfi comparisons (their saved reports stand) and the `logs/audit-followup-*` instrumented runs beyond the four modes above.

`scripts/audit_followup.py` extracts current-log cases and independently validates their forcing certificates. `data/audit_followup.txt` is the small replay corpus. `examples/audit_followup.rs` runs the real engine on it. `scripts/audit_search_modes.py` creates an instrumented copy under the ignored `logs/` directory and changes only the selected experimental policy; it does not edit production files. `docs/audit-followup-cases.json` retains boards, certificates and engine responses. `docs/audit-followup-experiments.json` retains representative instrumented counts and timings, also saved as stdout under `logs/audit-followup-*.txt`. Instrumentation changes the build and adds a little overhead; the production replays separately confirm the same failures. Mate distances and exact timing conventions differ across engines, so raw PentaZen and Rapfi reports are preserved.
