use high5::protocol::Engine;
use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    let mut engine = Engine::new();
    for line in stdin.lock().lines() {
        for reply in engine.handle(&line?) {
            writeln!(out, "{reply}")?;
        }
        out.flush()?;
        if engine.finished() {
            break;
        }
    }
    Ok(())
}
