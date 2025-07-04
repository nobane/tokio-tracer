// examples/basic-file.rs
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, fmt::Debug, hash::Hash};
use tokio::sync::mpsc;
use tokio_tracer::{Matcher, MatcherSet, TraceEvent, Tracer, TracerConfig};

/// LogType enum that defines all possible log file types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum LogType {
    Info,
    Debug,
    Error,
    Silenced,
    Dropped,
}

impl LogType {
    fn as_str(&self) -> &'static str {
        match self {
            LogType::Info => "info",
            LogType::Debug => "debug",
            LogType::Error => "error",
            LogType::Silenced => "silenced",
            LogType::Dropped => "dropped",
        }
    }

    fn file_name(&self) -> String {
        format!("{}.log", self.as_str())
    }
}

/// Message type to be sent through the channel
enum FileLogCommand {
    AddMessage {
        log_type: LogType,
        event: TraceEvent,
    },
    ClearLog(LogType),
    ClearAllLogs,
}

/// An improved file logger using the traced log events
struct FileLogger {
    file: Arc<Mutex<File>>,
    log_count: usize,
}

impl FileLogger {
    fn new(path: PathBuf) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            log_count: 0,
        })
    }

    fn handle_event(&mut self, event: TraceEvent) -> io::Result<()> {
        let formatted = format!("[{}] {}\n", event.id, event.format());

        {
            let mut file = self.file.lock().unwrap();
            file.write_all(formatted.as_bytes())?;
            file.flush()?;
        }

        self.log_count += 1;
        Ok(())
    }

    fn clear_log(&mut self) -> io::Result<()> {
        let mut file = self.file.lock().unwrap();
        file.set_len(0)?; // Truncate file
        file.flush()?;
        self.log_count = 0;
        Ok(())
    }
}

/// Main file trace demo that manages multiple log files
struct FileTraceDemo {
    loggers: HashMap<LogType, FileLogger>,
    base_path: PathBuf,
}

impl FileTraceDemo {
    fn new(base_path: PathBuf) -> io::Result<Self> {
        let mut demo = Self {
            loggers: HashMap::new(),
            base_path,
        };

        // Initialize all loggers
        for log_type in [
            LogType::Info,
            LogType::Debug,
            LogType::Error,
            LogType::Silenced,
            LogType::Dropped,
        ] {
            let log_path = demo.base_path.join(log_type.file_name());
            let logger = FileLogger::new(log_path)?;
            demo.loggers.insert(log_type, logger);
        }

        Ok(demo)
    }

    fn handle_message(&mut self, log_type: LogType, event: TraceEvent) -> io::Result<()> {
        if let Some(logger) = self.loggers.get_mut(&log_type) {
            logger.handle_event(event)?;
        }
        Ok(())
    }

    fn clear_log(&mut self, log_type: LogType) -> io::Result<()> {
        if let Some(logger) = self.loggers.get_mut(&log_type) {
            logger.clear_log()?;
        }
        Ok(())
    }

    fn clear_all_logs(&mut self) -> io::Result<()> {
        for (_, logger) in self.loggers.iter_mut() {
            logger.clear_log()?;
        }
        Ok(())
    }

    fn get_log_count(&self, log_type: LogType) -> usize {
        self.loggers
            .get(&log_type)
            .map(|logger| logger.log_count)
            .unwrap_or(0)
    }

    fn get_log_file_size(&self, log_type: LogType) -> io::Result<u64> {
        let path = self.base_path.join(log_type.file_name());
        let metadata = std::fs::metadata(path)?;
        Ok(metadata.len())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create a temporary directory for logs
    let temp_path = {
        let temp_dir = tempfile::tempdir()?;
        temp_dir.path().to_owned()
    };

    tokio::fs::create_dir_all(&temp_path).await?;

    println!("=== LOGGING TO TEMPORARY DIRECTORY ===");
    println!("Log directory: {}", temp_path.display());
    println!("=======================================\n");

    // Create filter sets for different log categories
    let info_matcher = MatcherSet::from_matcher(Matcher::info().all_modules());
    let debug_matcher = MatcherSet::from_matcher(Matcher::debug().all_modules());
    let error_matcher = MatcherSet::from_matcher(Matcher::error().all_modules());

    // Create config with all tabs
    let config = TracerConfig::default_main_tab()
        .with_tab(LogType::Info.as_str(), info_matcher)
        .with_tab(LogType::Debug.as_str(), debug_matcher)
        .with_tab(LogType::Error.as_str(), error_matcher);

    // Initialize the tracer with the config
    let tracer = Tracer::init(config)?;

    // Set up the unbounded channel for logger commands
    let (tx, mut rx) = mpsc::unbounded_channel::<FileLogCommand>();

    // Create demo logger
    let file_trace_demo = Arc::new(Mutex::new(FileTraceDemo::new(temp_path.clone())?));

    // Print the exact paths to each log file
    for log_type in [LogType::Info, LogType::Debug, LogType::Error] {
        println!(
            "{} log: {}",
            log_type.as_str(),
            temp_path.join(log_type.file_name()).display()
        );
    }
    println!("\n");

    // Spawn a task to process incoming log commands
    let demo_clone = Arc::clone(&file_trace_demo);
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            let mut demo = demo_clone.lock().unwrap();
            match cmd {
                FileLogCommand::AddMessage { log_type, event } => {
                    if let Err(e) = demo.handle_message(log_type, event) {
                        eprintln!("Error handling event for {}: {}", log_type.as_str(), e);
                    }
                }
                FileLogCommand::ClearLog(log_type) => {
                    if let Err(e) = demo.clear_log(log_type) {
                        eprintln!("Error clearing {} log: {}", log_type.as_str(), e);
                    }
                }
                FileLogCommand::ClearAllLogs => {
                    if let Err(e) = demo.clear_all_logs() {
                        eprintln!("Error clearing all logs: {e}");
                    }
                }
            }
        }
    });

    // Set callback for captured events
    let tx_clone = tx.clone();
    tracer
        .set_callback(move |event, tab_names| {
            // Create a copy of the event for each tab that captured it
            for &tab in tab_names {
                let log_type = match tab {
                    "info" => LogType::Info,
                    "debug" => LogType::Debug,
                    "error" => LogType::Error,
                    _ => unreachable!(),
                };

                if let Err(e) = tx_clone.send(FileLogCommand::AddMessage {
                    log_type,
                    event: Arc::clone(&event),
                }) {
                    eprintln!("Failed to send captured event: {e}");
                }
            }
        })?
        .await??;

    // Set callback for silenced events
    let tx_clone = tx.clone();
    tracer
        .set_silenced_callback(move |event, _silencers| {
            // We only send to the Silenced log file regardless of which tabs silenced it
            if let Err(e) = tx_clone.send(FileLogCommand::AddMessage {
                log_type: LogType::Silenced,
                event,
            }) {
                eprintln!("Failed to send silenced event: {e}");
            }
        })?
        .await??;

    // Set callback for dropped events
    let tx_clone = tx.clone();
    tracer
        .set_dropped_callback(move |event| {
            if let Err(e) = tx_clone.send(FileLogCommand::AddMessage {
                log_type: LogType::Dropped,
                event,
            }) {
                eprintln!("Failed to send dropped event: {e}");
            }
        })?
        .await??;

    // Generate example log events
    tokio::spawn(async move {
        let mut counter = 0;
        loop {
            tracing::trace!("events starting");
            tracing::info!("Regular info event {}", counter);
            tracing::trace!("fired first event");
            tracing::debug!("Debug details for count {}", counter);
            tracing::trace!("debug fired");

            if counter % 3 == 0 {
                tracing::warn!("Warning: counter is divisible by 3!");
            }

            if counter % 5 == 0 {
                tracing::error!("Error: counter hit multiple of 5!");
            }

            counter += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }
    });

    // Spawn another task with a different target
    tokio::spawn(async move {
        let mut counter = 0;
        loop {
            tracing::info!(
                target: "background_task",
                "Background task running {}",
                counter
            );

            tracing::debug!(
                target: "background_task",
                "Background task details {}",
                counter
            );

            if counter % 4 == 0 {
                tracing::warn!(
                    target: "background_task",
                    "Background warning!"
                );
            }

            counter += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
        }
    });

    // Create command channel for dynamic reconfiguration
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        // After 10 seconds, clear the info log
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        println!("\n=== CLEARING INFO LOG ===");
        if let Err(e) = tx_clone.send(FileLogCommand::ClearLog(LogType::Info)) {
            eprintln!("Failed to send clear info log command: {e}");
        }

        // After another 10 seconds, clear all logs
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        println!("\n=== CLEARING ALL LOGS ===");
        if let Err(e) = tx_clone.send(FileLogCommand::ClearAllLogs) {
            eprintln!("Failed to send clear all logs command: {e}");
        }
    });

    // Display stats periodically and show log file sizes
    let demo_clone = Arc::clone(&file_trace_demo);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

            println!("\n=== TRACER METRICS ===");
            println!("Captured events: {}", tracer.get_captured_count());
            println!("Silenced events: {}", tracer.get_silenced_count());
            println!("Dropped events: {}", tracer.get_dropped_count());

            // Display log file sizes and event counts
            println!("\n=== LOG FILE STATS ===");
            let demo = demo_clone.lock().unwrap();
            for log_type in [
                LogType::Info,
                LogType::Debug,
                LogType::Error,
                LogType::Silenced,
                LogType::Dropped,
            ] {
                let size = demo.get_log_file_size(log_type).unwrap_or(0);
                let count = demo.get_log_count(log_type);
                println!(
                    "{} log: {} bytes, {} events",
                    log_type.as_str(),
                    size,
                    count
                );
            }
            println!("=====================\n");
        }
    });

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;

    // Clean up
    println!("Shutting down...");
    println!("Logs remain available at: {}", temp_path.display());

    Ok(())
}
