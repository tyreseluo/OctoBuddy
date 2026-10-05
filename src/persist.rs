//! What waits survives a restart. The loops' processes do not (Claude Code
//! and octos are OctoBuddy's children), but what waited for them does: the
//! outer loop's queue and what it had not answered, the reports it had not
//! read, the notes, the peers held for their wave, each peer's line and the
//! message its turn was answering, the files not yet committed. They are
//! saved with the store, in each session (`Waiting`) and peer, and put back
//! when OctoBuddy starts, which then goes on: a cut-off turn starts again
//! from its message, and the outer loop gets what waited for it.
use crate::dispatch::{self, Delivery, From, Line};
use crate::model::{Store, Waited, WaitedReport, Waiting};
use crate::orchestrate::{OuterItem, OuterWork};
use crate::{i18n, OctoBuddyView};

fn waited(d: &Delivery) -> Waited {
    let kind = if d.from == From::Lead { "lead" } else { "person" };
    Waited { kind: kind.into(), id: Some(d.id.clone()), text: d.text.clone() }
}

fn delivery(w: &Waited) -> Delivery {
    let from = if w.kind == "person" { From::Person } else { From::Lead };
    match &w.id {
        Some(id) => {
            dispatch::seen_id(id);
            Delivery { from, text: w.text.clone(), id: id.clone() }
        }
        None => Delivery::new(from, w.text.clone()),
    }
}

impl OctoBuddyView {
    /// The store with what waits in memory written into it: what is saved.
    pub(crate) fn snapshot(&self) -> Store {
        let mut store = self.store.clone();
        let rt = &self.rt;
        for session in store.projects.iter_mut().flat_map(|p| p.sessions.iter_mut()) {
            let id = session.id.clone();
            let peers: Vec<String> = session.peers().iter().map(|p| p.id.clone()).collect();
            let outer = rt.outer_queue.get(&id).map(|q| q.iter().map(|item| match &item.work {
                OuterWork::Person(text) => Waited { kind: "person".into(), id: Some(item.id.clone()), text: text.clone() },
                OuterWork::Reports => Waited { kind: "reports".into(), id: Some(item.id.clone()), text: String::new() },
                OuterWork::Note(text) => Waited { kind: "note".into(), id: Some(item.id.clone()), text: text.clone() },
                OuterWork::Steered(text) => Waited { kind: "steered".into(), id: Some(item.id.clone()), text: text.clone() },
            }).collect()).unwrap_or_default();
            let waiting = Waiting {
                outer,
                outer_inflight: rt.lead_inflight.get(&id).map(|q| q.iter().cloned().collect()).unwrap_or_default(),
                reports: rt.unreported.get(&id).map(|v| v.iter().map(|(peer, report, forwarded)| {
                    WaitedReport { peer: peer.clone(), report: report.clone(), forwarded: *forwarded }
                }).collect()).unwrap_or_default(),
                notes: rt.notes.get(&id).cloned().unwrap_or_default(),
                held: peers.iter().filter_map(|p| rt.held.get(p).map(|dir| Waited { kind: p.clone(), id: None, text: dir.clone() })).collect(),
                rounds_left: self.rounds_left.get(&id).copied().filter(|n| *n > 0),
                pending: peers.iter().filter(|p| rt.pending.contains(*p)).cloned().collect(),
                person_tasks: Some(peers.iter().filter(|p| rt.person_tasks.contains(*p)).cloned().collect::<Vec<_>>()).filter(|v| !v.is_empty()),
            };
            session.waiting = (!waiting.is_empty()).then_some(waiting);
            for peer in session.peers_mut() {
                peer.queued = rt.lines.get(&peer.id).map(|l| l.queue.iter().map(waited).collect::<Vec<_>>()).filter(|q| !q.is_empty());
                peer.inflight = rt.inflight.get(&peer.id).map(waited);
                peer.uncommitted = rt.to_commit.get(&peer.id).cloned().filter(|f| !f.is_empty());
            }
        }
        store
    }

    /// Puts back what waited when OctoBuddy last saved (before the store
    /// marks cut-off turns interrupted); what must go on starts once the
    /// view is up (`resume`).
    pub(crate) fn restore(&mut self) -> Vec<String> {
        let mut resumed = Vec::new();
        for (pi, project) in self.store.projects.iter_mut().enumerate() {
            for (si, session) in project.sessions.iter_mut().enumerate() {
                let id = session.id.clone();
                if let Some(w) = session.waiting.take() {
                    let mut queue: std::collections::VecDeque<OuterItem> = w.outer.iter().map(|item| {
                        let work = match item.kind.as_str() {
                            "person" => OuterWork::Person(item.text.clone()),
                            "reports" => OuterWork::Reports,
                            "steered" => OuterWork::Steered(item.text.clone()),
                            _ => OuterWork::Note(item.text.clone()),
                        };
                        let id = item.id.clone().inspect(|i| dispatch::seen_id(i)).unwrap_or_else(|| Delivery::new(From::Lead, "").id);
                        OuterItem { id, work }
                    }).collect();
                    // What it was answering goes first, again.
                    for text in w.outer_inflight.iter().rev() {
                        let again = format!("(OctoBuddy restarted before you finished answering this. Answer it now.)\n\n{text}");
                        queue.push_front(OuterItem { id: Delivery::new(From::Lead, "").id, work: OuterWork::Note(again) });
                    }
                    if !queue.is_empty() {
                        resumed.push(format!("{pi}:{si}"));
                        self.rt.outer_queue.insert(id.clone(), queue);
                    }
                    if !w.reports.is_empty() {
                        self.rt.unreported.insert(id.clone(), w.reports.into_iter().map(|r| (r.peer, r.report, r.forwarded)).collect());
                    }
                    if !w.notes.is_empty() {
                        self.rt.notes.insert(id.clone(), w.notes);
                    }
                    for h in w.held {
                        self.rt.held.insert(h.kind, h.text);
                    }
                    if let Some(n) = w.rounds_left {
                        self.rounds_left.insert(id.clone(), n);
                    }
                    self.rt.pending.extend(w.pending);
                    self.rt.person_tasks.extend(w.person_tasks.unwrap_or_default());
                }
                for peer in session.peers_mut() {
                    let mut line = Line::default();
                    if let Some(queued) = peer.queued.take() {
                        line.queue = queued.iter().map(delivery).collect();
                    }
                    // A turn the restart cut off starts again, from its message.
                    if let Some(cut) = peer.inflight.take().filter(|_| peer.is_active()) {
                        let mut d = delivery(&cut);
                        d.text = format!("OctoBuddy restarted and cut your turn off. Look at the files as they are now, then go on with this:\n\n{}", d.text);
                        line.queue.push_front(d);
                    }
                    if !line.queue.is_empty() && peer.status != "closed" {
                        self.rt.lines.insert(peer.id.clone(), line);
                    }
                    if let Some(files) = peer.uncommitted.take() {
                        self.rt.to_commit.insert(peer.id.clone(), files);
                    }
                }
            }
        }
        resumed
    }

    /// Starts what the restart left waiting: each peer's next message, and
    /// the outer loop's queue; says so in each session it touches.
    pub(crate) fn resume(&mut self) {
        let lines: Vec<(String, Delivery)> = self.rt.lines.iter_mut()
            .filter(|(peer, l)| !l.is_busy() && !self.rt.held.contains_key(*peer))
            .filter_map(|(peer, l)| l.queue.pop_front().map(|d| (peer.clone(), d)))
            .collect();
        let mut told = std::collections::HashSet::new();
        for (peer, d) in lines {
            if let Some(at) = self.store.find_peer(&peer) {
                if told.insert(at) {
                    self.system(at, i18n::t("OctoBuddy restarted: the inner loops go on with what they were doing and what waited for them (Stop halts them).",
                        "OctoBuddy 已重启：inner 继续之前的工作和排队的消息（按停止可中止）。"));
                }
            }
            self.deliver(&peer, d, crate::dispatch::Mode::Queue);
        }
        let sessions: Vec<crate::model::SessionRef> = self.store.projects.iter().enumerate()
            .flat_map(|(pi, p)| p.sessions.iter().enumerate().filter(|(_, s)| self.rt.outer_queue.get(&s.id).is_some_and(|q| !q.is_empty())).map(move |(si, _)| (pi, si)))
            .collect();
        for at in sessions {
            if told.insert(at) {
                self.system(at, i18n::t("OctoBuddy restarted: the outer loop goes on with what waited for it (Stop halts it).",
                    "OctoBuddy 已重启：outer 继续处理排队的工作（按停止可中止）。"));
            }
            self.drain_outer(at);
        }
        // Waves held when it stopped: started now if their earlier waves are
        // settled (a check that once kept them waiting may have changed).
        let held: Vec<crate::model::SessionRef> = self.rt.held.keys().filter_map(|peer| self.store.find_peer(peer)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        for at in held {
            self.release_waves(at);
        }
    }
}
