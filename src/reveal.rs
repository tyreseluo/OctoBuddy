//! A reply being written, shown at an even pace. The agent's text arrives in
//! bursts (a token or a hundred at once); drawn as it comes, it jumps. Here
//! a live message shows a growing prefix of what has arrived, a word at a
//! time: each frame it gains an eighth of what it is behind by (at least a
//! couple of characters), so a burst rolls out over about eight frames —
//! close to Cindy's words appearing 16 ms apart, no word waiting more than
//! about 160 ms — and a slow stream is never held back. No caret (Cindy
//! draws none either); before any text, three dots wave. A code fence still
//! open is closed for the drawing, so a half-written block lays out as code.

/// Characters a live message gains each frame: at least `MIN_STEP`, and a
/// share of what it is behind by, so it catches up in about 1 / `CATCH_UP`
/// frames (about a tenth of a second at 60 frames a second).
const MIN_STEP: f64 = 2.0;
const CATCH_UP: f64 = 0.125;

/// Text that was already there when a message was first seen live (the view
/// opened on it mid-reply): shown at once but for its last this many
/// characters, which roll out.
const TAIL: usize = 60;

/// What a live message shows before its first word: three dots, one lit,
/// the light moving along (`phase` 0, 1, 2).
pub fn thinking(phase: usize) -> &'static str {
    ["●  ○  ○", "○  ●  ○", "○  ○  ●"][phase % 3]
}

/// Where a live message's reveal is: characters shown, and how many there are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reveal {
    pub shown: f64,
    pub total: usize,
}

impl Reveal {
    /// First seen with `total` characters already there.
    pub fn start(total: usize) -> Reveal {
        Reveal { shown: total.saturating_sub(TAIL) as f64, total }
    }

    /// One frame on; whether it moved.
    pub fn step(&mut self) -> bool {
        let behind = self.total as f64 - self.shown;
        if behind <= 0.0 {
            self.shown = self.total as f64;
            return false;
        }
        self.shown += (behind * CATCH_UP).max(MIN_STEP).min(behind);
        true
    }

    #[cfg(test)]
    pub fn caught_up(&self) -> bool {
        self.shown >= self.total as f64
    }
}

/// The placeholder a live row carries before its text (`…`, or ` …` after
/// it): the caret stands for it.
pub fn without_placeholder(body: &str) -> &str {
    let body = body.trim_end();
    body.strip_suffix('…').map(str::trim_end).unwrap_or(body)
}

/// The first `n` characters of `text`, as a live message draws them: up to
/// the end of the word it cuts into (a word is never shown half), with an
/// open code fence closed after them.
pub fn visible(text: &str, n: usize) -> String {
    let mut cut = text.char_indices().nth(n).map(|(i, _)| i).unwrap_or(text.len());
    // Inside a Latin word or a number: on to its end.
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    if text[..cut].chars().next_back().is_some_and(word) {
        cut += text[cut..].char_indices().find(|(_, c)| !word(*c)).map(|(i, _)| i).unwrap_or(text.len() - cut);
    }
    let mut out = text[..cut].to_string();
    let fences = out.lines().filter(|l| l.trim_start().starts_with("```")).count();
    if fences % 2 == 1 {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("```");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_rolls_out_and_catches_up() {
        let mut r = Reveal { shown: 0.0, total: 300 };
        let mut frames = 0;
        while r.step() {
            frames += 1;
            assert!(r.shown <= 300.0);
        }
        assert!(r.caught_up());
        // Neither all at once nor dragging on: a few dozen frames at most.
        assert!((5..=60).contains(&frames), "{frames} frames");
        // A trickle keeps up: a character a frame is shown as it comes.
        let mut r = Reveal { shown: 10.0, total: 11 };
        assert!(r.step());
        assert!(r.caught_up());
    }

    #[test]
    fn a_message_seen_late_shows_most_of_itself_at_once() {
        assert_eq!(Reveal::start(1000).shown, 940.0);
        assert_eq!(Reveal::start(20).shown, 0.0);
    }

    #[test]
    fn half_a_code_block_is_drawn_as_code() {
        let text = "Here:\n```rust\nfn main() {";
        assert_eq!(visible(text, 100), "Here:\n```rust\nfn main() {\n```");
        assert_eq!(visible("done.\n```\nx\n```\nok", 100), "done.\n```\nx\n```\nok");
        // Cut by characters, not bytes; a Chinese character is a word.
        assert_eq!(visible("记一笔", 2), "记一");
    }

    #[test]
    fn a_word_is_never_shown_half() {
        assert_eq!(visible("hello world", 3), "hello");
        assert_eq!(visible("hello world", 6), "hello ");
        assert_eq!(visible("v12.5 done", 2), "v12");
    }

    #[test]
    fn the_dots_wave() {
        assert_ne!(thinking(0), thinking(1));
        assert_eq!(thinking(3), thinking(0));
    }

    #[test]
    fn the_placeholder_gives_way_to_the_caret() {
        assert_eq!(without_placeholder("…"), "");
        assert_eq!(without_placeholder("working on it …"), "working on it");
        assert_eq!(without_placeholder("done"), "done");
    }
}
