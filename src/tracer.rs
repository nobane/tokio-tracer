// src/tracer.rs
use anyhow::{Context, Result};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, oneshot};

use crate::{
    DispatcherCommand, MatcherSet, ResultSender, TraceCounters, TraceEvent, TracerConfig,
    TracingDispatcher, TracingSubscriber,
};

pub type EventCallback = Arc<dyn Fn(TraceEvent, &[&str]) + Send + Sync>;
pub type SilencedEventCallback = Arc<dyn Fn(TraceEvent, &[&str]) + Send + Sync>;
pub type DroppedEventCallback = Arc<dyn Fn(TraceEvent) + Send + Sync>;

pub struct Tracer {
    event_tx: mpsc::UnboundedSender<TraceEvent>,
    command_tx: mpsc::UnboundedSender<DispatcherCommand>,
    counters: TraceCounters,
}

impl Tracer {
    /// Initialize tracing with default configuration
    pub fn init_default() -> Result<Self> {
        Self::init(TracerConfig::default_main_tab())
    }

    /// Initialize tracing with the provided config
    pub fn init(config: TracerConfig) -> Result<Self> {
        let tracer = Self::new_with_config(config);

        // Create our custom subscriber
        let subscriber =
            TracingSubscriber::new(tracer.event_tx.clone(), tracer.counters.event_id.clone());

        // Set the global default subscriber
        tracing::subscriber::set_global_default(subscriber)
            .context("Failed to set global default subscriber")?;

        Ok(tracer)
    }

    pub fn new_with_config(config: TracerConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let counters = TraceCounters::default();

        // Create a map of tabs from the config
        let mut tabs = HashMap::new();
        for tab in config.tabs {
            tabs.insert(tab.name, tab.matcher_set);
        }

        // Create and start the dispatcher with initial tabs
        let dispatcher = TracingDispatcher::new(event_rx, command_rx, counters.clone(), tabs);

        // Start the dispatcher with a self-consuming run method
        tokio::spawn(dispatcher.run());

        Self {
            event_tx,
            command_tx,
            counters,
        }
    }

    #[doc(hidden)]
    pub fn _get_sender_for_testing(&self) -> mpsc::UnboundedSender<TraceEvent> {
        self.event_tx.clone()
    }

    /// Get statistics about captured events
    pub fn get_captured_count(&self) -> u64 {
        self.counters.get_captured_count()
    }

    /// Get statistics about silenced events
    pub fn get_silenced_count(&self) -> u64 {
        self.counters.get_silenced_count()
    }

    /// Get statistics about dropped events
    pub fn get_dropped_count(&self) -> u64 {
        self.counters.get_dropped_count()
    }

    pub fn set_stdout_callback(&self) -> Result<()> {
        self.set_callback(|event, tab_names| {
            let tab = if tab_names.len() == 1 {
                tab_names[0]
            } else {
                &tab_names.join(", ")
            };

            println!("[{tab}]: {}", event.format_full());
        })?;

        Ok(())
    }

    /// Set a callback for handling captured trace events
    pub fn set_callback<F>(&self, callback: F) -> Result<oneshot::Receiver<Result<()>>>
    where
        F: Fn(TraceEvent, &[&str]) + Send + Sync + 'static,
    {
        let callback = Arc::new(callback);
        let (response_tx, response_rx) = oneshot::channel();
        let result_sender = ResultSender::new(response_tx);

        self.command_tx
            .send(DispatcherCommand::SetCallback(callback, result_sender))
            .context("Failed to send set_callback command")?;

        Ok(response_rx)
    }

    /// Add a new tab
    pub fn add_tab(
        &self,
        name: impl Into<String>,
        matcher_set: MatcherSet,
    ) -> Result<oneshot::Receiver<Result<()>>> {
        let (response_tx, response_rx) = oneshot::channel();
        let result_sender = ResultSender::new(response_tx);

        self.command_tx
            .send(DispatcherCommand::AddTab(
                name.into(),
                matcher_set,
                result_sender,
            ))
            .context("Failed to send add_tab command")?;

        Ok(response_rx)
    }

    /// Clear event statistics
    pub fn clear_stats(&self) -> Result<oneshot::Receiver<Result<()>>> {
        let (response_tx, response_rx) = oneshot::channel();
        let result_sender = ResultSender::new(response_tx);

        self.command_tx
            .send(DispatcherCommand::ClearStats(result_sender))
            .context("Failed to send clear_stats command")?;

        Ok(response_rx)
    }

    pub fn set_silenced_callback<F>(&self, callback: F) -> Result<oneshot::Receiver<Result<()>>>
    where
        F: Fn(TraceEvent, &[&str]) + Send + Sync + 'static,
    {
        let callback = Arc::new(callback);
        let (response_tx, response_rx) = oneshot::channel();
        let result_sender = ResultSender::new(response_tx);

        self.command_tx
            .send(DispatcherCommand::SetSilencedCallback(
                callback,
                result_sender,
            ))
            .context("Failed to send set_silenced_callback command")?;

        Ok(response_rx)
    }

    pub fn set_dropped_callback<F>(&self, callback: F) -> Result<oneshot::Receiver<Result<()>>>
    where
        F: Fn(TraceEvent) + Send + Sync + 'static,
    {
        let callback = Arc::new(callback);
        let (response_tx, response_rx) = oneshot::channel();
        let result_sender = ResultSender::new(response_tx);

        self.command_tx
            .send(DispatcherCommand::SetUncapturedCallback(
                callback,
                result_sender,
            ))
            .context("Failed to send set_dropped_callback command")?;

        Ok(response_rx)
    }

    /// Update an existing tab's filter set
    pub fn update_tab(
        &self,
        name: impl Into<String>,
        matcher_set: MatcherSet,
    ) -> Result<oneshot::Receiver<Result<()>>> {
        let (response_tx, response_rx) = oneshot::channel();
        let result_sender = ResultSender::new(response_tx);

        self.command_tx
            .send(DispatcherCommand::UpdateTab(
                name.into(),
                matcher_set,
                result_sender,
            ))
            .context("Failed to send update_tab command")?;

        Ok(response_rx)
    }

    /// Remove a tab by name
    pub fn remove_tab(&self, name: impl Into<String>) -> Result<oneshot::Receiver<Result<()>>> {
        let (response_tx, response_rx) = oneshot::channel();
        let result_sender = ResultSender::new(response_tx);

        self.command_tx
            .send(DispatcherCommand::RemoveTab(name.into(), result_sender))
            .context("Failed to send remove_tab command")?;

        Ok(response_rx)
    }
}
