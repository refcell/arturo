//! TUI application state.

use crate::{
    participant::{ParticipantView, SharedParticipant},
    status::{SidecarStatus, StatusReceiver},
};

/// Application state for the TUI.
pub struct App {
    /// All participants.
    participants: Vec<SharedParticipant>,
    /// Cached views for rendering.
    views: Vec<ParticipantView>,
    /// Status receiver for sidecar updates.
    status_rx: StatusReceiver,
    /// Cached sidecar status.
    status: SidecarStatus,
    /// Whether the application should quit.
    should_quit: bool,
}

impl App {
    /// Creates a new application.
    pub fn new(participants: Vec<SharedParticipant>, status_rx: StatusReceiver) -> Self {
        let views = Vec::with_capacity(participants.len());
        Self {
            participants,
            views,
            status_rx,
            status: SidecarStatus::default(),
            should_quit: false,
        }
    }

    /// Updates the cached views from participants.
    pub async fn update_views(&mut self) {
        self.views.clear();
        for participant in &self.participants {
            self.views.push(participant.get_view().await);
        }
        // Update sidecar status
        self.status = self.status_rx.borrow().clone();
    }

    /// Returns the cached views.
    pub fn views(&self) -> &[ParticipantView] {
        &self.views
    }

    /// Returns the current sidecar status.
    pub fn status(&self) -> &SidecarStatus {
        &self.status
    }

    /// Sets the quit flag.
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Returns whether the application should quit.
    pub const fn should_quit(&self) -> bool {
        self.should_quit
    }
}
