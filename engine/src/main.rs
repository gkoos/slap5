use slap5::protocol::Engine;
use std::io::{self, BufRead, Write};

/// A timestamped record of what the engine read and wrote, enabled by the `SLAP5_TRACE` environment
/// variable (a file path). A frozen game is otherwise indistinguishable from a frozen harness: with
/// this, a `recv` line with no matching `sent` means the engine did not answer, and a `sent` line with
/// no reply in Piskvork's own pipe log means Piskvork never took it. Off by default, so the engine
/// writes nothing and behaves exactly as before.
struct Trace {
    file: std::fs::File,
    pid: u32,
}

impl Trace {
    fn new(path: &str) -> Option<Trace> {
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok()?;
        Some(Trace { file, pid: std::process::id() })
    }

    fn log(&mut self, dir: &str, line: &str) {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(self.file, "{ms} pid={} {dir} {line}", self.pid);
        let _ = self.file.flush();
    }
}

/// The search switches the engine was started with: the command line first, then `SLAP5_FLAGS` split on
/// whitespace. Piskvork starts the engines itself and passes its environment through - the trace mechanism
/// relies on the same fact - so the environment is the only way a pool run can vary the search settings
/// without a rebuild, which is what makes an A/B of switches one command (`match.ps1 -Flags`).
fn flags<I: IntoIterator<Item = String>>(args: I, env: Option<String>) -> Vec<String> {
    let mut out: Vec<String> = args.into_iter().collect();
    if let Some(s) = env {
        out.extend(s.split_whitespace().map(str::to_owned));
    }
    out
}

fn main() -> io::Result<()> {
    let mut trace = std::env::var("SLAP5_TRACE").ok().and_then(|p| Trace::new(&p));
    if let Some(t) = trace.as_mut() {
        t.log("info", "engine start");
    }
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    let mut engine = Engine::new();
    // A typo in a run's switches must not pass silently: it would make the run look like a default run.
    for flag in flags(std::env::args().skip(1), std::env::var("SLAP5_FLAGS").ok()) {
        if engine.configure(&flag) {
            if let Some(t) = trace.as_mut() {
                t.log("info", &format!("switch {flag}"));
            }
        } else if let Some(t) = trace.as_mut() {
            t.log("info", &format!("unknown switch {flag}"));
        } else {
            eprintln!("slap5: unknown switch {flag}");
        }
    }
    for line in stdin.lock().lines() {
        let line = line?;
        if let Some(t) = trace.as_mut() {
            t.log("recv", &line);
        }
        let replies = engine.handle(&line);
        for reply in &replies {
            writeln!(out, "{reply}")?;
        }
        // The flush is what puts the bytes on Piskvork's pipe; the trace line comes after it, so a
        // blocked write shows up as a `recv` with no matching `sent`.
        out.flush()?;
        if let Some(t) = trace.as_mut() {
            for reply in &replies {
                t.log("sent", reply);
            }
            if engine.finished() {
                t.log("info", "engine stop");
            }
        }
        if engine.finished() {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The switches come from the command line and then from the environment, in that order: the command
    // line is what a person at a shell sets, the environment is what a pool run can set (`match.ps1 -Flags`).
    #[test]
    fn flags_take_the_command_line_then_the_environment() {
        let got = flags(
            vec!["no-threes".to_owned(), "wide=6".to_owned()],
            Some("clustered-tt aspiration\n  no-futility ".to_owned()),
        );
        assert_eq!(got, vec!["no-threes", "wide=6", "clustered-tt", "aspiration", "no-futility"]);
        assert!(flags(Vec::<String>::new(), None).is_empty());
    }

    // Every switch the bundle uses has to be one the engine knows. A rejected switch is reported on the
    // trace, because otherwise the run would quietly be a different run from the one that was asked for.
    #[test]
    fn the_bundle_switches_are_known_and_reach_the_settings() {
        let mut engine = Engine::new();
        let bundle = "root-history wide-openings clustered-tt aspiration";
        for flag in flags(Vec::new(), Some(bundle.to_owned())) {
            assert!(engine.configure(&flag), "unknown switch {flag}");
        }
        assert!(!engine.configure("no-such-switch"));
    }
}
