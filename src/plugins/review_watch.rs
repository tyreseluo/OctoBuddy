//! A submitted app's review, watched: a reviewer's new comment on its App
//! Hub issue becomes a request to fix it, waiting for the person.

use crate::OctoBuddyView;

impl OctoBuddyView {
    /// The clock's tick: the watched reviews that are due.
    pub(crate) fn review_watch_tick(&mut self) {}
}
