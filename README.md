# high5

A freestyle gomoku engine in Rust that speaks the Piskvork/Gomocup protocol. It currently plays legal games with a placeholder move chooser; the search is not written yet.

## Build and test

```
cargo build --release
cargo test
cargo bench
```

On x86-64 the build uses `target-cpu=native` (see `.cargo/config.toml`), which selects the AVX2 board. Other targets use the scalar board.

## Play

The executable is `target/release/pbrain-high5.exe`. In Piskvork, open Players > Settings (F4) and select it. Piskvork must be set to a 15×15 board; the engine rejects other sizes.

Piskvork keeps its board size in the registry (`height` and `sirka`, the width). To set 15×15 from PowerShell:

```
$k = 'HKCU:\Software\Petr Lastovicka\piskvorky'
New-Item -Path $k -Force | Out-Null
Set-ItemProperty $k -Name height -Value 15 -Type DWord
Set-ItemProperty $k -Name sirka -Value 15 -Type DWord
```

Delete the key to restore the default (20×20). Batch mode (`piskvork.exe -p <engine1> <engine2>`) has no board-size option, so it uses this setting too.

## Layout

- `src/bitboard.rs`, `src/avx.rs`: board representation
- `src/threats.rs`: threat masks
- `src/game.rs`, `src/protocol.rs`, `src/mover.rs`, `src/main.rs`: game state and Piskvork protocol
- `docs/algorithm.md`: the algorithm and what is implemented
- `docs/design-decisions.md`: why things are the way they are
