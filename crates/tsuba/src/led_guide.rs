//! `tsuba led-guide`: a guided, one-LED-at-a-time check of which lighting-grid position is
//! which media key (previous, play/pause, next, mute) on the BlackWidow V4 Pro. The user runs it
//! (it lights LEDs on the device) and answers what they saw; the result is the exact content
//! for `tools/media-leds.json`, the single place the media LEDs are defined.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use anyhow::{bail, Result};

/// The media keys, in the order of `tools/media-leds.json`.
pub const KEYS: [(&str, &str); 4] = [("previous", "Previous track"), ("play_pause", "Play / Pause"), ("next", "Next track"), ("mute", "Mute")];

/// Positions tried by default: the suspects first (the two keys that did not light), then the
/// neighbours, all on row 0 of the 9×24 grid.
pub const DEFAULT_COLS: [u8; 8] = [20, 22, 21, 19, 23, 18, 24, 17];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// One of the four media keys (index into [`KEYS`]).
    Key(usize),
    /// Some other key / LED lit.
    Other,
    /// Nothing visible.
    Nothing,
    /// Show it again.
    Repeat,
    /// Stop asking.
    Quit,
}

pub fn parse_answer(s: &str) -> Option<Answer> {
    Some(match s.trim().to_ascii_lowercase().as_str() {
        "p" | "prev" | "previous" => Answer::Key(0),
        "y" | "pp" | "play" | "pause" | "playpause" | "play/pause" => Answer::Key(1),
        "n" | "next" => Answer::Key(2),
        "m" | "mute" => Answer::Key(3),
        "o" | "other" => Answer::Other,
        "x" | "none" | "nothing" | "-" => Answer::Nothing,
        "r" | "repeat" | "again" | "" => Answer::Repeat,
        "q" | "quit" | "exit" => Answer::Quit,
        _ => return None,
    })
}

/// What the answers say.
#[derive(Debug, PartialEq)]
pub struct Summary {
    /// Content for `tools/media-leds.json` (keys not found keep the current value).
    pub json: String,
    /// Keys no position was reported for.
    pub missing: Vec<&'static str>,
    /// Keys reported for more than one position (the later answer wins).
    pub repeated: Vec<&'static str>,
}

/// `answers`: (grid column on row 0, what the user saw). `current`: the present values.
pub fn summarise(answers: &[(u8, Answer)], current: &BTreeMap<&str, [u8; 2]>) -> Summary {
    let mut found: BTreeMap<usize, u8> = BTreeMap::new();
    let mut repeated = Vec::new();
    for (col, a) in answers {
        if let Answer::Key(k) = a {
            if found.insert(*k, *col).is_some() {
                repeated.push(KEYS[*k].0);
            }
        }
    }
    let mut parts = Vec::new();
    let mut missing = Vec::new();
    for (i, (key, _)) in KEYS.iter().enumerate() {
        let rc = match found.get(&i) {
            Some(c) => [0, *c],
            None => {
                missing.push(*key);
                current.get(key).copied().unwrap_or([0, 19 + i as u8])
            }
        };
        parts.push(format!("\"{key}\": [{}, {}]", rc[0], rc[1]));
    }
    Summary { json: format!("{{{}}}", parts.join(", ")), missing, repeated }
}

/// Run the guide. `show(lit, seconds)` lights the given LEDs white on a dark keyboard for that
/// long and then restores the user's lighting.
pub fn run(
    cols: &[u8],
    seconds: u64,
    current: &BTreeMap<&str, [u8; 2]>,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    show: &mut dyn FnMut(u8, u64) -> Result<()>,
) -> Result<Summary> {
    writeln!(out, "Media-key LED guide. For each position I light ONE LED white (everything else dark) for {seconds} s,")?;
    writeln!(out, "then put your lighting back. Watch the keyboard, then answer which key lit:")?;
    writeln!(out, "  p = Previous track   y = Play / Pause   n = Next track   m = Mute")?;
    writeln!(out, "  o = some other key   x = nothing lit     r = show it again   q = stop here\n")?;
    let mut answers: Vec<(u8, Answer)> = Vec::new();
    'cols: for col in cols {
        loop {
            writeln!(out, "Position (row 0, column {col}) ...")?;
            out.flush()?;
            show(*col, seconds)?;
            let answer = loop {
                write!(out, "Which key lit? [p/y/n/m/o/x/r/q] ")?;
                out.flush()?;
                let mut line = String::new();
                if input.read_line(&mut line)? == 0 {
                    bail!("no answer (standard input closed): run this in a terminal");
                }
                match parse_answer(&line) {
                    Some(a) => break a,
                    None => writeln!(out, "Please answer one of p, y, n, m, o, x, r, q.")?,
                }
            };
            match answer {
                Answer::Repeat => continue,
                Answer::Quit => break 'cols,
                a => {
                    answers.push((*col, a));
                    break;
                }
            }
        }
    }
    let s = summarise(&answers, current);
    writeln!(out)?;
    if !s.missing.is_empty() {
        writeln!(out, "No position was reported for: {} (their current values are kept).", s.missing.join(", "))?;
    }
    if !s.repeated.is_empty() {
        writeln!(out, "More than one position was reported for: {} (the last answer is used).", s.repeated.join(", "))?;
    }
    writeln!(out, "Put this line in tools/media-leds.json, then regenerate the pictures:\n")?;
    writeln!(out, "{}\n", s.json)?;
    writeln!(out, "  python3 tools/gen-device-art.py          (Tsuba's built-in drawing)")?;
    writeln!(out, "  python3 tools/patch-pack-leds.py         (your personal picture pack, if you have one)")?;
    writeln!(out, "Then rebuild / restart Tsuba. Or just send the line above to the developer.")?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current() -> BTreeMap<&'static str, [u8; 2]> {
        BTreeMap::from([("previous", [0, 19]), ("play_pause", [0, 20]), ("next", [0, 21]), ("mute", [0, 22])])
    }

    #[test]
    fn answers_parse() {
        assert_eq!(parse_answer("y\n"), Some(Answer::Key(1)));
        assert_eq!(parse_answer(" Mute "), Some(Answer::Key(3)));
        assert_eq!(parse_answer("x"), Some(Answer::Nothing));
        assert_eq!(parse_answer("\n"), Some(Answer::Repeat));
        assert_eq!(parse_answer("banana"), None);
    }

    #[test]
    fn summary_replaces_only_what_was_found() {
        let s = summarise(&[(20, Answer::Nothing), (23, Answer::Key(1)), (22, Answer::Other), (18, Answer::Key(3))], &current());
        assert_eq!(s.json, r#"{"previous": [0, 19], "play_pause": [0, 23], "next": [0, 21], "mute": [0, 18]}"#);
        assert_eq!(s.missing, vec!["previous", "next"]);
        assert!(s.repeated.is_empty());
        // The line is valid JSON for tools/media-leds.json.
        let v: BTreeMap<String, [u8; 2]> = serde_json_like(&s.json);
        assert_eq!(v["play_pause"], [0, 23]);
    }

    fn serde_json_like(s: &str) -> BTreeMap<String, [u8; 2]> {
        // Tiny parser for the fixed shape above (the CLI has no serde_json dependency).
        let mut m = BTreeMap::new();
        for part in s.trim_matches(|c| c == '{' || c == '}').split("], ") {
            let (k, v) = part.split_once(": [").unwrap();
            let nums: Vec<u8> = v.trim_end_matches(']').split(", ").map(|n| n.parse().unwrap()).collect();
            m.insert(k.trim_matches('"').to_string(), [nums[0], nums[1]]);
        }
        m
    }

    #[test]
    fn guide_walks_the_positions_and_repeats_on_request() {
        let mut shown = Vec::new();
        let mut input = std::io::Cursor::new(b"r\nbanana\ny\nx\nm\nq\n".to_vec());
        let mut out = Vec::new();
        let s = run(&[20, 22, 21, 19], 1, &current(), &mut input, &mut out, &mut |c, _| {
            shown.push(c);
            Ok(())
        })
        .unwrap();
        // 20 shown twice (repeat), 22, 21, then quit at 21? answers: r -> repeat 20, banana (reprompt), y -> 20 = Play/Pause,
        // x -> 22 nothing, m -> 21 = Mute, q -> stop before 19.
        assert_eq!(shown, vec![20, 20, 22, 21, 19]);
        assert_eq!(s.json, r#"{"previous": [0, 19], "play_pause": [0, 20], "next": [0, 21], "mute": [0, 21]}"#);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Please answer one of"));
        assert!(text.contains("tools/media-leds.json"));
    }

    #[test]
    fn closed_input_is_an_error() {
        let mut input = std::io::Cursor::new(Vec::new());
        let mut out = Vec::new();
        assert!(run(&[20], 1, &current(), &mut input, &mut out, &mut |_, _| Ok(())).is_err());
    }
}
