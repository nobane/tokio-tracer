// src/tracing_dispatcher.rs

use anyhow::{Result, anyhow};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{mpsc, oneshot};

use crate::{DroppedEventCallback, EventCallback, MatcherSet, SilencedEventCallback, TraceEvent};

pub(crate) enum DispatcherCommand {
    SetCallback(EventCallback, ResultSender),
    SetSilencedCallback(SilencedEventCallback, ResultSender),
    SetUncapturedCallback(DroppedEventCallback, ResultSender),
    AddTab(String, MatcherSet, ResultSender),
    UpdateTab(String, MatcherSet, ResultSender),
    RemoveTab(String, ResultSender),
    ClearStats(ResultSender),
}

pub(crate) struct TracingDispatcher {
    event_rx: mpsc::UnboundedReceiver<TraceEvent>,
    command_rx: mpsc::UnboundedReceiver<DispatcherCommand>,
    counters: TraceCounters,
    tabs: HashMap<String, MatcherSet>,
    callback: Option<EventCallback>,
    silenced_callback: Option<SilencedEventCallback>,
    dropped_callback: Option<DroppedEventCallback>,
    pending_captured_events: Vec<(TraceEvent, Vec<String>)>,
}

impl TracingDispatcher {
    pub fn new(
        event_rx: mpsc::UnboundedReceiver<TraceEvent>,
        command_rx: mpsc::UnboundedReceiver<DispatcherCommand>,
        counters: TraceCounters,
        initial_tabs: HashMap<String, MatcherSet>,
    ) -> Self {
        Self {
            event_rx,
            command_rx,
            counters,
            tabs: initial_tabs,
            callback: None,
            silenced_callback: None,
            dropped_callback: None,
            pending_captured_events: Vec::new(),
        }
    }

    // Main run loop that consumes self
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some(event) = self.event_rx.recv() => {
                    self.handle_event(event);
                },

                Some(cmd) = self.command_rx.recv() => {
                    self.handle_command(cmd).await;
                },

                else => break,
            }
        }
    }

    fn handle_event(&mut self, event: TraceEvent) {
        let mut captured_by = Vec::new();
        let mut silenced_by = Vec::new();

        // Check each tab
        for (name, filter) in &self.tabs {
            // First check if any exclusion filters apply - these take precedence
            let mut is_silenced = false;

            for matcher in filter.iter_matchers() {
                if !matcher.include && matcher.matches(&event) {
                    silenced_by.push(name.clone());
                    is_silenced = true;
                    break;
                }
            }

            // If not silenced, check if any inclusion filters match
            if !is_silenced {
                for matcher in filter.iter_matchers() {
                    if matcher.include && matcher.matches(&event) {
                        captured_by.push(name.clone());
                        break;
                    }
                }
            }
        }

        // Determine status and update counters
        if !captured_by.is_empty() {
            self.counters.captured.fetch_add(1, Ordering::SeqCst);
            if let Some(cb) = &self.callback {
                // Collect references to tab names
                let tab_refs: Vec<&str> = captured_by.iter().map(String::as_str).collect();
                cb(Arc::clone(&event), &tab_refs);
            } else {
                // Store event for later processing when callback is set
                self.pending_captured_events
                    .push((Arc::clone(&event), captured_by));
            }
        } else if !silenced_by.is_empty() {
            self.counters.silenced.fetch_add(1, Ordering::SeqCst);
            if let Some(silenced_cb) = &self.silenced_callback {
                // Collect references to silencer names
                let silencer_refs: Vec<&str> = silenced_by.iter().map(String::as_str).collect();
                silenced_cb(Arc::clone(&event), &silencer_refs);
            }
        } else {
            self.counters.dropped.fetch_add(1, Ordering::SeqCst);
            if let Some(dropped_cb) = &self.dropped_callback {
                dropped_cb(Arc::clone(&event));
            }
        }
    }

    fn handle_set_callback(&mut self, cb: EventCallback, response_tx: ResultSender) {
        // Set the new callback
        self.callback = Some(cb);

        // Drain pending captured events
        if let Some(callback) = &self.callback {
            for (event, tabs) in self.pending_captured_events.drain(..) {
                // Convert to references for the callback
                let tab_refs: Vec<&str> = tabs.iter().map(String::as_str).collect();
                callback(event, &tab_refs);
            }
        }
        response_tx.success();
    }

    fn handle_set_silenced_callback(
        &mut self,
        cb: SilencedEventCallback,
        response_tx: ResultSender,
    ) {
        // Set the new callback
        self.silenced_callback = Some(cb);
        response_tx.success();
    }

    fn handle_set_dropped_callback(&mut self, cb: DroppedEventCallback, response_tx: ResultSender) {
        // Set the new callback
        self.dropped_callback = Some(cb);
        response_tx.success();
    }

    // Handle a dispatcher command
    async fn handle_command(&mut self, cmd: DispatcherCommand) {
        match cmd {
            DispatcherCommand::SetCallback(cb, response_tx) => {
                self.handle_set_callback(cb, response_tx);
            }
            DispatcherCommand::SetSilencedCallback(cb, response_tx) => {
                self.handle_set_silenced_callback(cb, response_tx);
            }
            DispatcherCommand::SetUncapturedCallback(cb, response_tx) => {
                self.handle_set_dropped_callback(cb, response_tx);
            }
            DispatcherCommand::AddTab(name, filter_set, response_tx) => {
                self.handle_add_tab(name, filter_set, response_tx);
            }
            DispatcherCommand::UpdateTab(name, filter_set, response_tx) => {
                self.handle_update_tab(name, filter_set, response_tx);
            }
            DispatcherCommand::RemoveTab(name, response_tx) => {
                self.handle_remove_tab(name, response_tx);
            }
            DispatcherCommand::ClearStats(response_tx) => {
                self.handle_clear_stats(response_tx);
            }
        }
    }

    // Simplified add_tab handler
    fn handle_add_tab(
        &mut self,
        name: impl Into<String>,
        filter_set: MatcherSet,
        response_tx: ResultSender,
    ) {
        // Add the tab to the map
        self.tabs.insert(name.into(), filter_set);
        response_tx.success();
    }

    // Simplified update_tab handler
    fn handle_update_tab(
        &mut self,
        name: impl AsRef<str>,
        filter_set: MatcherSet,
        response_tx: ResultSender,
    ) {
        let name = name.as_ref();
        if !self.tabs.contains_key(name) {
            response_tx.error(format!("Subscriber '{name:?}' not found"));
            return;
        }

        // Update the filter set
        self.tabs.insert(name.to_string(), filter_set);
        response_tx.success();
    }

    // Simplified remove_tab handler
    fn handle_remove_tab(&mut self, name: impl AsRef<str>, response_tx: ResultSender) {
        let name = name.as_ref();
        if !self.tabs.contains_key(name) {
            response_tx.error(format!("Subscriber '{name:?}' not found"));
            return;
        }

        // Remove the tab
        self.tabs.remove(name);
        response_tx.success();
    }

    // Simplified clear_stats handler
    fn handle_clear_stats(&mut self, response_tx: ResultSender) {
        // Reset statistics
        self.counters.clear();
        response_tx.success();
    }
}

// Result sender for operation responses
pub struct ResultSender(oneshot::Sender<Result<()>>);

impl ResultSender {
    pub fn new(sender: oneshot::Sender<Result<()>>) -> Self {
        Self(sender)
    }

    pub fn success(self) {
        let _ = self.0.send(Ok(()));
    }

    pub fn error(self, msg: impl Into<String>) {
        let _ = self.0.send(Err(anyhow!(msg.into())));
    }
}

// Statistics counters struct
#[derive(Default, Clone)]
pub(crate) struct TraceCounters {
    pub event_id: Arc<AtomicU64>,
    pub captured: Arc<AtomicU64>,
    pub silenced: Arc<AtomicU64>,
    pub dropped: Arc<AtomicU64>,
}

impl TraceCounters {
    fn clear(&self) {
        self.captured.store(0, Ordering::SeqCst);
        self.silenced.store(0, Ordering::SeqCst);
        self.dropped.store(0, Ordering::SeqCst);
    }

    // Return captured count
    pub fn get_captured_count(&self) -> u64 {
        self.captured.load(Ordering::SeqCst)
    }

    // Get silenced count
    pub fn get_silenced_count(&self) -> u64 {
        self.silenced.load(Ordering::SeqCst)
    }

    // Get dropped count
    pub fn get_dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::SeqCst)
    }
}
