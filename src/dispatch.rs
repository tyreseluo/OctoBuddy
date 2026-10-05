//! Delivering messages to a peer: queue or interrupt.
//!
//! octos runs one turn at a time per session and refuses a second, so each
//! peer has a line here: the turn it runs, if any, and what waits after it.
//! `Queue` waits its turn. `Steer` joins the running turn at its next step
//! (octos's `turn/steer`; what the turn ends without reading comes back and
//! waits in line). `Interrupt` replaces the current task: it goes to
//! the front of the line and the running turn is cancelled (octos discards
//! an interrupted turn, input included), so it is the very next thing the
//! peer reads.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Queue,
    Steer,
    Interrupt,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s.to_ascii_lowercase().as_str() {
            "interrupt" => Mode::Interrupt,
            "steer" => Mode::Steer,
            _ => Mode::Queue,
        }
    }
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Interrupt => "interrupt",
            Mode::Steer => "steer",
            Mode::Queue => "queue",
        }
    }
}

/// Who a message comes from: it decides who hears about the reply.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum From {
    Lead,
    Person,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Delivery {
    pub from: From,
    pub text: String,
    /// Names it while it waits (`q7`), so the lead can change or drop it.
    pub id: String,
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A restored message keeps its id: new ones are numbered after it.
pub fn seen_id(id: &str) {
    if let Some(n) = id.strip_prefix('q').and_then(|n| n.parse::<u64>().ok()) {
        NEXT_ID.fetch_max(n + 1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Delivery {
    pub fn new(from: From, text: impl Into<String>) -> Delivery {
        let n = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Delivery { from, text: text.into(), id: format!("q{n}") }
    }
}

/// What the caller must do now.
#[derive(Debug, PartialEq)]
pub enum Step {
    /// Start a turn with this message.
    Start(Delivery),
    /// Cancel this running turn; the message waits at the front.
    Interrupt(String),
    /// Add this message to the running turn.
    Steer(Delivery),
    /// Nothing yet: it waits in the line.
    Wait,
}

#[derive(Debug, Default)]
pub struct Line {
    /// The session is open: turns can start.
    pub ready: bool,
    /// The running turn, and whom it answers.
    pub turn: Option<(String, From)>,
    pub queue: VecDeque<Delivery>,
    /// An interrupt was sent for the running turn and has not ended it yet.
    interrupting: bool,
}

impl Line {
    pub fn is_busy(&self) -> bool {
        self.turn.is_some()
    }

    pub fn deliver(&mut self, d: Delivery, mode: Mode) -> Step {
        if !self.ready {
            match mode {
                Mode::Interrupt => self.queue.push_front(d),
                Mode::Queue | Mode::Steer => self.queue.push_back(d),
            }
            return Step::Wait;
        }
        match (&self.turn, mode) {
            (None, _) => Step::Start(d),
            (Some(_), Mode::Steer) if !self.interrupting => Step::Steer(d),
            (Some(_), Mode::Queue | Mode::Steer) => {
                self.queue.push_back(d);
                Step::Wait
            }
            (Some((turn, _)), Mode::Interrupt) => {
                let turn = turn.clone();
                self.queue.push_front(d);
                if self.interrupting {
                    Step::Wait
                } else {
                    self.interrupting = true;
                    Step::Interrupt(turn)
                }
            }
        }
    }

    /// The caller started a turn for `from`.
    pub fn started(&mut self, turn: String, from: From) {
        self.turn = Some((turn, from));
    }

    /// The running turn ended; returns whom it answered and what to start next.
    pub fn ended(&mut self) -> (Option<From>, Option<Delivery>) {
        let from = self.turn.take().map(|(_, f)| f);
        self.interrupting = false;
        (from, self.queue.pop_front())
    }

    /// The session opened: the first thing in line may start.
    pub fn opened(&mut self) -> Option<Delivery> {
        self.ready = true;
        if self.turn.is_none() { self.queue.pop_front() } else { None }
    }

    /// octos refused a start (a turn was running after all): back to the front.
    pub fn refused(&mut self, d: Delivery) {
        self.turn = None;
        self.queue.push_front(d);
    }

    /// Steers the turn ended without reading: first in line, in order.
    pub fn dropped(&mut self, from: From, texts: Vec<String>) {
        for text in texts.into_iter().rev() {
            self.queue.push_front(Delivery::new(from, text));
        }
    }

    /// The lead edits what it queued (never the person's messages).
    /// `Err` says why nothing changed.
    pub fn cancel(&mut self, id: &str) -> Result<(), String> {
        let at = self.own(id)?;
        self.queue.remove(at);
        Ok(())
    }

    pub fn replace(&mut self, id: &str, text: &str) -> Result<(), String> {
        let at = self.own(id)?;
        self.queue[at].text = text.to_string();
        Ok(())
    }

    /// Several queued messages become one, where the first of them was.
    pub fn merge(&mut self, ids: &[String], text: &str) -> Result<(), String> {
        if ids.len() < 2 {
            return Err("merge needs at least two ids".into());
        }
        let mut at: Vec<usize> = ids.iter().map(|id| self.own(id)).collect::<Result<_, _>>()?;
        at.sort_unstable();
        self.queue[at[0]].text = text.to_string();
        for i in at[1..].iter().rev() {
            self.queue.remove(*i);
        }
        Ok(())
    }

    /// The person moves a waiting message up (`-1`) or down (`1`) the line:
    /// any message, theirs or the lead's (after Cindy's queue panel).
    pub fn shift(&mut self, id: &str, by: isize) -> bool {
        let Some(at) = self.queue.iter().position(|d| d.id == id) else { return false };
        let to = at as isize + by;
        if to < 0 || to as usize >= self.queue.len() {
            return false;
        }
        self.queue.swap(at, to as usize);
        true
    }

    /// The person drops a waiting message; says what it was.
    pub fn drop_waiting(&mut self, id: &str) -> Option<Delivery> {
        let at = self.queue.iter().position(|d| d.id == id)?;
        self.queue.remove(at)
    }

    fn own(&self, id: &str) -> Result<usize, String> {
        match self.queue.iter().position(|d| d.id == id) {
            None => Err(format!("{id} is not waiting (it may have started already)")),
            Some(i) if self.queue[i].from != From::Lead => Err(format!("{id} is the person's message: only they can change it")),
            Some(i) => Ok(i),
        }
    }

    /// Stop: forget what waits; returns the running turn to cancel.
    pub fn clear(&mut self) -> Option<String> {
        self.queue.clear();
        self.interrupting = self.turn.is_some();
        self.turn.as_ref().map(|(t, _)| t.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Delivery {
        Delivery { from: From::Lead, text: text.into(), id: format!("id-{text}") }
    }

    #[test]
    fn queue_waits_for_the_running_turn() {
        let mut line = Line { ready: true, ..Default::default() };
        assert_eq!(line.deliver(d("a"), Mode::Queue), Step::Start(d("a")));
        line.started("T1".into(), From::Lead);
        assert_eq!(line.deliver(d("b"), Mode::Queue), Step::Wait);
        assert_eq!(line.deliver(d("c"), Mode::Queue), Step::Wait);
        assert_eq!(line.ended(), (Some(From::Lead), Some(d("b"))));
        assert_eq!(line.queue, VecDeque::from([d("c")]));
    }

    #[test]
    fn interrupt_cancels_once_and_goes_first() {
        let mut line = Line { ready: true, ..Default::default() };
        line.started("T1".into(), From::Lead);
        assert_eq!(line.deliver(d("later"), Mode::Queue), Step::Wait);
        assert_eq!(line.deliver(d("new plan"), Mode::Interrupt), Step::Interrupt("T1".into()));
        // a second interrupt before the first lands does not cancel again
        assert_eq!(line.deliver(d("newer plan"), Mode::Interrupt), Step::Wait);
        let (_, next) = line.ended();
        assert_eq!(next, Some(d("newer plan")), "the latest replacement is next");
        assert_eq!(line.queue, VecDeque::from([d("new plan"), d("later")]));
    }

    #[test]
    fn steer_joins_a_running_turn_and_dropped_steers_wait() {
        let mut line = Line { ready: true, ..Default::default() };
        assert_eq!(line.deliver(d("a"), Mode::Steer), Step::Start(d("a")), "idle: a steer is a start");
        line.started("T1".into(), From::Lead);
        assert_eq!(line.deliver(d("more"), Mode::Steer), Step::Steer(d("more")));
        line.dropped(From::Lead, vec!["x".into(), "y".into()]);
        assert_eq!(line.ended().1.map(|d| d.text), Some("x".into()));
        assert_eq!(line.queue.iter().map(|d| d.text.as_str()).collect::<Vec<_>>(), ["y"]);
    }

    #[test]
    fn before_the_session_opens_everything_waits() {
        let mut line = Line::default();
        assert_eq!(line.deliver(d("brief"), Mode::Queue), Step::Wait);
        assert_eq!(line.deliver(d("urgent"), Mode::Interrupt), Step::Wait);
        assert_eq!(line.opened(), Some(d("urgent")));
        line.started("T1".into(), From::Lead);
        assert_eq!(line.ended().1, Some(d("brief")));
    }

    #[test]
    fn the_lead_edits_only_its_own_queued_messages() {
        let mut line = Line { ready: true, ..Default::default() };
        line.started("T".into(), From::Lead);
        for t in ["a", "b", "c"] {
            line.deliver(d(t), Mode::Queue);
        }
        line.deliver(Delivery { from: From::Person, text: "p".into(), id: "id-p".into() }, Mode::Queue);
        assert!(line.cancel("id-p").unwrap_err().contains("person"));
        line.replace("id-a", "A").unwrap();
        line.merge(&["id-c".into(), "id-b".into()], "B+C").unwrap();
        line.cancel("id-A").unwrap_err();
        let left: Vec<_> = line.queue.iter().map(|d| d.text.as_str()).collect();
        assert_eq!(left, ["A", "B+C", "p"]);
        assert!(line.merge(&["id-a".into()], "x").is_err());
        assert!(line.cancel("id-zzz").unwrap_err().contains("not waiting"));
    }

    #[test]
    fn the_person_reorders_and_drops_any_waiting_message() {
        let mut line = Line { ready: true, ..Default::default() };
        line.started("T".into(), From::Lead);
        line.deliver(d("a"), Mode::Queue);
        line.deliver(Delivery { from: From::Person, text: "p".into(), id: "id-p".into() }, Mode::Queue);
        line.deliver(d("c"), Mode::Queue);
        assert!(line.shift("id-c", -1) && !line.shift("id-a", -1), "the first cannot go higher");
        assert!(!line.shift("id-zzz", 1));
        let order: Vec<_> = line.queue.iter().map(|d| d.text.as_str()).collect();
        assert_eq!(order, ["a", "c", "p"]);
        assert_eq!(line.drop_waiting("id-a").map(|d| d.text), Some("a".into()), "the lead's too");
        assert_eq!(line.ended().1.map(|d| d.text), Some("c".into()), "the new first runs next");
    }

    #[test]
    fn a_refused_start_goes_back_to_the_front() {
        let mut line = Line { ready: true, ..Default::default() };
        line.started("T1".into(), From::Person);
        line.refused(d("x"));
        assert!(!line.is_busy());
        assert_eq!(line.queue.front(), Some(&d("x")));
        assert_eq!(Mode::parse("INTERRUPT"), Mode::Interrupt);
        assert_eq!(Mode::parse("steer"), Mode::Steer);
        assert_eq!(Mode::parse("whatever"), Mode::Queue);
    }
}
