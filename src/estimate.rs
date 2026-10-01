//! Estimates against what happened. OctoBuddy measures each finished slice
//! (the rounds agent-estimation gave it, the time its turns took, the model
//! calls it made) and folds a project's history into one calibration. The
//! outer loop is given it with each request, to scale its next estimate;
//! the views show a slice's estimate, the calibrated one, and the real time.
use crate::model::{Peer, Session};

/// The minutes one estimated round is taken to be (agent-estimation's
/// round: one write-run-verify cycle).
pub const MIN_PER_ROUND: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calibration {
    /// How many finished slices it is drawn from.
    pub n: usize,
    /// What an estimated round really took, in minutes (median).
    pub min_per_round: f64,
    /// And in model calls (median).
    pub steps_per_round: f64,
}

impl Calibration {
    /// Real time over estimated time.
    pub fn factor(&self) -> f64 {
        self.min_per_round / MIN_PER_ROUND
    }

    /// The line the outer loop is given.
    pub fn line(&self) -> String {
        format!("CALIBRATION (OctoBuddy measured {} finished slice(s) of this project): one round you estimate took {:.1} min \
and {:.0} model calls (median), {:.1}× the {MIN_PER_ROUND:.0} min a round is taken to be. When you estimate with \
agent-estimation, scale its figures by this and give the calibrated estimate.", self.n, self.min_per_round, self.steps_per_round, self.factor())
    }
}

/// The time a peer's turns on the lead's tasks took, in minutes; a turn
/// still running counts up to `now`.
pub fn actual_minutes(p: &Peer, now: u64) -> Option<f64> {
    let turns: Vec<f64> = p.log().iter().filter(|e| e.from == "lead").map(|e| match e.took {
        Some(t) => t as f64,
        None if e.outcome.is_none() && p.is_active() => now.saturating_sub(e.at) as f64,
        None => 0.0,
    }).collect();
    (!turns.is_empty()).then(|| turns.iter().sum::<f64>() / 60.0)
}

/// What finished slices of `sessions` say about their estimates; `None`
/// before there are two to go by.
pub fn calibrate<'a>(sessions: impl IntoIterator<Item = &'a Session>) -> Option<Calibration> {
    let mut minutes = Vec::new();
    let mut steps = Vec::new();
    for p in sessions.into_iter().flat_map(|s| s.peers()) {
        let Some(est) = p.estimate.filter(|e| *e > 0.0) else { continue };
        // A slice the lead planned and accepted: a blocked, stopped or
        // failed one says nothing about how long the work takes.
        let accepted = p.review.as_deref().is_some_and(|r| r.trim_start().starts_with("accept"));
        if p.reviews_for.is_some() || p.by_person == Some(true) || !accepted {
            continue;
        }
        let Some(m) = actual_minutes(p, 0).filter(|m| *m > 0.0) else { continue };
        minutes.push(m / est);
        if let Some(used) = p.rounds_used {
            steps.push(used as f64 / est);
        }
    }
    if minutes.len() < 2 {
        return None;
    }
    let median = |v: &mut Vec<f64>| -> f64 {
        v.sort_by(|a, b| a.total_cmp(b));
        let n = v.len();
        if n == 0 { 0.0 } else if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
    };
    Some(Calibration { n: minutes.len(), min_per_round: median(&mut minutes), steps_per_round: median(&mut steps) })
}

/// A length in minutes, short: `45s`, `6m`, `1h 05m`.
pub fn minutes_text(m: f64) -> String {
    let s = (m * 60.0).max(0.0).round() as u64;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", (s + 30) / 60),
        _ => format!("{}h {:02}m", s / 3600, (s % 3600 + 30) / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Exchange, Store};

    fn slice(slug: &str, est: f64, took: u64, steps: u64) -> Peer {
        let mut p = crate::chat::tests::peer(vec![Exchange {
            from: "lead".into(), input: "do".into(), reply: Some("done".into()), outcome: Some("completed".into()),
            at: 100, steps: None, took: Some(took), cost_total: None, began: None,
        }]);
        p.slug = slug.into();
        p.estimate = Some(est);
        p.rounds_used = Some(steps);
        p.review = Some("accept: done".into());
        p
    }

    #[test]
    fn finished_slices_calibrate_the_next_estimate() {
        let mut store = Store::default();
        let pi = store.add_project(&std::env::temp_dir().to_string_lossy()).unwrap();
        let at = store.add_session(pi).unwrap();
        store.session_mut(at).unwrap().peers_mut().push(slice("a", 1.0, 360, 10));
        assert_eq!(calibrate(store.projects[pi].sessions.iter()), None, "one slice is not enough");
        store.session_mut(at).unwrap().peers_mut().push(slice("b", 2.0, 600, 12));
        store.session_mut(at).unwrap().peers_mut().push(slice("c", 1.0, 240, 8));
        let c = calibrate(store.projects[pi].sessions.iter()).unwrap();
        // 6, 5 and 4 minutes a round: the median is 5.
        assert_eq!(c.n, 3);
        assert!((c.min_per_round - 5.0).abs() < 1e-9);
        assert!((c.steps_per_round - 8.0).abs() < 1e-9);
        assert!((c.factor() - 5.0 / 3.0).abs() < 1e-9);
        assert!(c.line().contains("CALIBRATION") && c.line().contains("agent-estimation"));
        // One that was stopped (never accepted) does not count.
        let mut stopped = slice("d", 20.0, 240, 30);
        stopped.review = None;
        store.session_mut(at).unwrap().peers_mut().push(stopped);
        assert_eq!(calibrate(store.projects[pi].sessions.iter()).unwrap().n, 3);
    }
}
