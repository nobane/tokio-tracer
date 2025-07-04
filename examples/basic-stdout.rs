// examples/basic-stdout.rs
use anyhow::Result;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, mpsc};
use tokio_tracer::{Matcher, MatcherSet, TraceEvent, Tracer, TracerConfig};
use tracing::{Level, debug, error, info, warn};

/// Message type to be sent through the channel
enum LogCommand {
    AddMessage { tab: String, event: TraceEvent },
    ClearTab(String),
}

/// Represents a logging tab in the TUI
struct LogTab {
    messages: Vec<TraceEvent>,
    formatted_messages: Vec<String>, // Pre-formatted messages for display
}

/// Main TUI logger that manages multiple tabs with different filtering
struct StdoutTracerDemo {
    tabs: HashMap<String, LogTab>,
    // Keep track of tab order
    tab_order: Vec<String>,
}

impl StdoutTracerDemo {
    fn new() -> Self {
        Self {
            tabs: HashMap::new(),
            tab_order: Vec::new(),
        }
    }

    fn add_tab(&mut self, tab: &str) {
        let tab_name = tab.to_string();
        self.tabs.insert(
            tab_name.clone(),
            LogTab {
                messages: Vec::new(),
                formatted_messages: Vec::new(),
            },
        );
        // Add to order list if not already present
        if !self.tab_order.contains(&tab_name) {
            self.tab_order.push(tab_name);
        }
    }

    fn add_message(&mut self, tab: &str, event: TraceEvent) {
        if let Some(tab_data) = self.tabs.get_mut(tab) {
            // Format the event for display
            let formatted = match tab {
                "silenced" => format!("[{}] [SILENCED] {}", event.id, event.format()),
                "dropped" => {
                    format!("[{}] [DROPPED] {}", event.id, event.format())
                }
                _ => format!("[{}] {}", event.id, event.format()),
            };

            // Store both original and formatted messages
            tab_data.messages.push(event);
            tab_data.formatted_messages.push(formatted);
        }
    }

    fn clear_tab(&mut self, tab: &str) {
        if let Some(tab_data) = self.tabs.get_mut(tab) {
            tab_data.messages.clear();
            tab_data.formatted_messages.clear();
        }
    }

    async fn render(&self) {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        println!("\n========================");
        println!("===  LOGGER   STATE  ===");
        println!("========================");

        // Display tabs in the order they were added
        for tab in &self.tab_order {
            if let Some(tab_data) = self.tabs.get(tab) {
                println!(
                    "Tab: {} ({} messages)",
                    tab,
                    tab_data.formatted_messages.len()
                );

                // Show all messages without collapsing (default)
                for msg in &tab_data.formatted_messages {
                    println!("{msg}");
                }
                println!("-----------------------\n");
            }
        }
    }
}

mod database {
    use super::*;
    use tracing::{Level, span, trace};

    pub fn do_event() {
        // Generate log messages within a database span
        let db_span = span!(Level::INFO, "database");
        let _enter = db_span.enter();

        trace!("do_event started!");
        info!("Database connection established");
        trace!("Connection details initialized");
        debug!("Database query executed: SELECT * FROM users");
        debug!("Query completed in 5ms");

        drop(_enter);
        trace!("Exited database span");

        warn!("System running low on memory");
        trace!("Memory warning logged");
        error!("Failed to process request");
        trace!("Error logged and reported");
    }
}

/// Display current tracer statistics
async fn display_stats(tracer: &Tracer) {
    println!("\n=== TRACER METRICS ===");
    println!("Captured messages: {}", tracer.get_captured_count());
    println!("Silenced messages: {}", tracer.get_silenced_count());
    println!("Dropped messages: {}", tracer.get_dropped_count());
    println!("=====================\n");
}

/// Update a tab's filter set with notification
async fn update_tab_matcher(
    tracer: &Tracer,
    command_tx: &mpsc::UnboundedSender<LogCommand>,
    tab: &str,
    filter_set: MatcherSet,
    description: &str,
) -> Result<()> {
    println!("\n~~~ UPDATING FILTER: {tab} ~~~");
    println!("{description}");

    // Clear the tab for the updated tab
    command_tx.send(LogCommand::ClearTab(tab.to_string()))?;

    // Update the tab's filter set
    tracer.update_tab(tab, filter_set)?.await??;

    // Regenerate events to populate tabs with new filter settings
    database::do_event();

    println!("Filter updated successfully!\n");
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    Ok(())
}

/// Streamlined function to update a tab with just a level
async fn update_tab_level(
    tracer: &Tracer,
    command_tx: &mpsc::UnboundedSender<LogCommand>,
    tab: &str,
    level: Level,
    include: bool,
) -> Result<()> {
    // Use appropriate module patterns for different tabs
    let module_patterns = match tab {
        "database" => vec!["*database*".to_string()],
        _ => vec!["*".to_string()], // Default pattern for Main, Errors, Warnings, etc.
    };

    let filter_set = if include {
        MatcherSet::from_matcher(Matcher::new(level).module_patterns(&module_patterns))
    } else {
        MatcherSet::from_matchers([
            Matcher::trace().all_modules(),
            Matcher::new(level)
                .exclude()
                .module_patterns(&module_patterns),
        ])
    };
    let inclusion = if include { "INCLUDE" } else { "EXCLUDE" };
    let description =
        format!("Updating {tab} tab to {inclusion} level {level} {module_patterns:?}",);

    update_tab_matcher(tracer, command_tx, tab, filter_set, &description).await
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create config with initial tabs
    let config = TracerConfig::from_tabs([
        ("main", Matcher::debug().all_modules()),
        (
            "database",
            Matcher::debug().include().module_pattern("*database*"),
        ),
        ("errors", Matcher::error().all_modules()),
        ("warnings", Matcher::warn().all_modules()),
    ]);

    // Initialize the tracer with the config
    let tracer = Tracer::init(config)?;

    // Set up the unbounded channel for logger commands
    let (tx, mut rx) = mpsc::unbounded_channel::<LogCommand>();

    // Create a TUI logger with collapsing disabled by default
    let logger = Arc::new(Mutex::new(StdoutTracerDemo::new()));

    // Setup tabs with different filter sets
    {
        let mut l = logger.lock().await;
        l.add_tab("main");
        l.add_tab("database");
        l.add_tab("errors");
        l.add_tab("warnings");
        l.add_tab("silenced"); // Tab for silenced messages
        l.add_tab("dropped"); // Tab for dropped messages
    }

    // Spawn a task to process incoming log commands
    let logger_clone = Arc::clone(&logger);
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            let mut logger = logger_clone.lock().await;
            match cmd {
                LogCommand::AddMessage { tab, event } => {
                    logger.add_message(&tab, event);
                }
                LogCommand::ClearTab(tab) => {
                    logger.clear_tab(&tab);
                }
            }
            drop(logger);
        }
    });

    // Set main callback that sends messages through the channel
    let tx_clone = tx.clone();
    tracer
        .set_callback(move |event, tab_names| {
            // Send a message for each tab that captured this event
            for &tab in tab_names {
                if let Err(e) = tx_clone.send(LogCommand::AddMessage {
                    tab: tab.to_string(),
                    event: Arc::clone(&event),
                }) {
                    eprintln!("Failed to send log event: {e}");
                }
            }
        })?
        .await??;

    // Set callback for silenced messages
    let tx_clone = tx.clone();
    tracer
        .set_silenced_callback(move |event, _silencers| {
            // We only need to log once to the Silenced tab, regardless of how many tabs silenced it
            if let Err(e) = tx_clone.send(LogCommand::AddMessage {
                tab: "silenced".to_string(),
                event,
            }) {
                eprintln!("Failed to send silenced event: {e}");
            }
        })?
        .await??;

    // Set callback for dropped messages
    let tx_clone = tx.clone();
    tracer
        .set_dropped_callback(move |event| {
            if let Err(e) = tx_clone.send(LogCommand::AddMessage {
                tab: "dropped".to_string(),
                event,
            }) {
                eprintln!("Failed to send dropped event: {e}");
            }
        })?
        .await??;

    // Generate initial trace events
    database::do_event();

    // Small delay to allow messages to be processed
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let do_output = || async {
        {
            let logger = logger.lock().await;
            logger.render().await;
        }

        display_stats(&tracer).await;
    };

    do_output().await;

    update_tab_level(&tracer, &tx, "database", Level::TRACE, true).await?;

    do_output().await;

    update_tab_level(&tracer, &tx, "database", Level::ERROR, true).await?;

    do_output().await;

    update_tab_level(&tracer, &tx, "database", Level::TRACE, false).await?;
    update_tab_level(&tracer, &tx, "main", Level::WARN, true).await?;

    do_output().await;

    update_tab_level(&tracer, &tx, "database", Level::WARN, true).await?;
    update_tab_level(&tracer, &tx, "main", Level::DEBUG, true).await?;
    update_tab_level(&tracer, &tx, "errors", Level::ERROR, true).await?;

    do_output().await;

    // Show final stats
    println!("\n=== FINAL TRACER STATS ===");
    display_stats(&tracer).await;

    Ok(())
}
