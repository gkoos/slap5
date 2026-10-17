use super::*;

fn run(e: &mut Engine, lines: &[&str]) -> Vec<String> {
    lines.iter().flat_map(|l| e.handle(l)).collect()
}

fn last(lines: &[String]) -> &str {
    lines.last().expect("a reply")
}

fn started() -> Engine {
    let mut e = Engine::new();
    assert_eq!(e.handle("START 15"), ["OK"]);
    e.handle("INFO timeout_turn 100");
    e
}

#[test]
fn start_sizes() {
    let mut e = Engine::new();
    assert_eq!(e.handle("START 15"), ["OK"]);
    assert!(e.handle("START 20")[0].starts_with("ERROR"));
    assert!(e.handle("RECTSTART 15,15")[0].starts_with("ERROR"));
}

#[test]
fn begin_plays_centre() {
    let mut e = started();
    assert_eq!(last(&e.handle("BEGIN")), "7,7");
}

#[test]
fn turn_returns_legal_move() {
    let mut e = started();
    let r = e.handle("TURN 7,7");
    assert!(r[..r.len() - 1].iter().all(|l| l.starts_with("DEBUG ")));
    let (x, y) = parse_xy(last(&r)).unwrap();
    assert!(x < 15 && y < 15 && (x, y) != (7, 7));
}

#[test]
fn rejects_bad_turns() {
    let mut e = started();
    e.handle("TURN 7,7");
    assert!(e.handle("TURN 7,7")[0].starts_with("ERROR"));
    assert!(e.handle("TURN 15,0")[0].starts_with("ERROR"));
    assert!(e.handle("TURN a,b")[0].starts_with("ERROR"));
}

#[test]
fn commands_before_start_fail() {
    let mut e = Engine::new();
    assert!(e.handle("BEGIN")[0].starts_with("ERROR"));
    assert!(e.handle("TURN 1,1")[0].starts_with("ERROR"));
}

#[test]
fn board_command() {
    let mut e = started();
    let r = run(&mut e, &["BOARD", "10,10,1", "10,11,2", "11,11,1", "9,10,2", "DONE"]);
    let (x, y) = parse_xy(last(&r)).unwrap();
    assert!(![(10, 10), (10, 11), (11, 11), (9, 10)].contains(&(x, y)));
}

#[test]
fn board_with_overlapping_stones_errors() {
    let mut e = started();
    let r = run(&mut e, &["BOARD", "1,1,1", "1,1,2", "DONE"]);
    assert!(r[0].starts_with("ERROR"));
}

#[test]
fn takeback_frees_cell() {
    let mut e = started();
    e.handle("TURN 3,3");
    assert_eq!(e.handle("TAKEBACK 3,3"), ["OK"]);
    assert!(e.handle("TAKEBACK 3,3")[0].starts_with("ERROR"));
    assert!(parse_xy(last(&e.handle("TURN 3,3"))).is_some());
}

#[test]
fn restart_clears_board() {
    let mut e = started();
    e.handle("TURN 7,7");
    assert_eq!(e.handle("RESTART"), ["OK"]);
    assert_eq!(last(&e.handle("BEGIN")), "7,7");
}

#[test]
fn plays_the_winning_move() {
    let mut e = started();
    let r = run(&mut e, &["BOARD", "3,7,1", "4,7,1", "5,7,1", "6,7,1", "10,10,2", "11,10,2", "DONE"]);
    assert!(["2,7", "7,7"].contains(&last(&r)), "{r:?}");
}

#[test]
fn time_budget() {
    assert_eq!(budget(Some(1000), None), Duration::from_millis(950));
    assert_eq!(budget(None, None), Duration::from_millis(950));
    assert_eq!(budget(Some(0), None), Duration::from_millis(20));
    assert_eq!(budget(Some(1000), Some(2000)), Duration::from_millis(150));
    assert_eq!(budget(Some(1000), Some(-500)), Duration::from_millis(20));
    assert_eq!(budget(Some(1000), Some(2_147_483_647)), Duration::from_millis(950));
}

#[test]
fn misc_commands() {
    let mut e = started();
    assert!(e.handle("INFO timeout_turn 1000").is_empty());
    assert!(e.handle("ABOUT")[0].starts_with("name=\"slap5\""));
    assert_eq!(e.handle("SWAP2BOARD"), ["UNKNOWN SWAP2BOARD"]);
    assert!(!e.finished());
    assert!(e.handle("END").is_empty());
    assert!(e.finished());
}
