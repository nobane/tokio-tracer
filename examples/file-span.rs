// examples/file-span.rs
use anyhow::Result;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tracer::{Matcher, MatcherSet, Tracer, TracerConfig};
use tracing::{Level, debug, error, info, span, trace, warn};

// Command to log messages or manage log files
struct LogMessage {
    destination: String,
    message: Arc<tokio_tracer::TraceData>,
}
// Logger that writes to file or stdout based on destination
struct DestinationLogger {
    file_loggers: std::collections::HashMap<String, Arc<Mutex<File>>>,
    log_counts: std::collections::HashMap<String, usize>,
    log_dir: PathBuf,
}

impl DestinationLogger {
    fn new(log_dir: PathBuf) -> io::Result<Self> {
        std::fs::create_dir_all(&log_dir)?;

        let mut file_loggers = std::collections::HashMap::new();
        let mut log_counts = std::collections::HashMap::new();

        // Initialize file loggers for each destination except stdout
        for dest in ["database", "network", "security"] {
            let file_path = log_dir.join(format!("{dest}.log"));
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&file_path)?;

            file_loggers.insert(dest.to_string(), Arc::new(Mutex::new(file)));
            log_counts.insert(dest.to_string(), 0);
        }

        // Initialize count for stdout
        log_counts.insert("stdout".to_string(), 0);

        Ok(Self {
            file_loggers,
            log_counts,
            log_dir,
        })
    }

    fn log_message(
        &mut self,
        destination: &str,
        event: Arc<tokio_tracer::TraceData>,
    ) -> io::Result<()> {
        let formatted = format!("{}\n", event.format_full());

        match destination {
            "stdout" => {
                print!("STDOUT: {formatted}");
                *self.log_counts.entry("stdout".to_string()).or_insert(0) += 1;
            }
            _ => {
                if let Some(file_lock) = self.file_loggers.get(destination) {
                    let mut file = file_lock.lock().unwrap();
                    file.write_all(formatted.as_bytes())?;
                    file.flush()?;
                    *self.log_counts.entry(destination.to_string()).or_insert(0) += 1;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("No logger for destination: {destination}"),
                    ));
                }
            }
        }

        Ok(())
    }

    fn print_stats(&self) {
        println!("\n=== LOGGING STATISTICS ===");
        for (dest, count) in &self.log_counts {
            let size = if dest != "stdout" {
                let path = self.log_dir.join(format!("{dest}.log"));
                std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
            } else {
                0 // Stdout has no file size
            };

            println!(
                "{} logs: {} messages{}",
                dest,
                count,
                if dest != "stdout" {
                    format!(" ({size} bytes)")
                } else {
                    String::new()
                }
            );
        }
        println!("=========================\n");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create a temporary directory for log files
    let log_dir = tempfile::tempdir()?.keep();
    println!("Logging to directory: {}", log_dir.display());

    // Define filter sets for different span-based routing

    // 1. Stdout filter - only logs that aren't in special spans
    let stdout_matcher = MatcherSet::from_matchers(vec![
        // Include all general logs without special spans
        Matcher::info().all_modules(),
        // Exclude logs from specific spans we want to redirect
        Matcher::trace()
            .exclude() // Explicitly exclude
            .span_patterns(vec!["database*", "network*", "security*"]),
    ]);

    // 2. Database logs filter
    let db_matcher = MatcherSet::from_matcher(
        Matcher::debug().all_modules().span_pattern("database*"), // Only database spans
    );

    // 3. Network logs filter
    let network_matcher = MatcherSet::from_matcher(
        Matcher::debug().all_modules().span_pattern("network*"), // Only network spans
    );

    // 4. Security logs filter - capture all levels for security events
    let security_matcher = MatcherSet::from_matcher(
        Matcher::trace() // ALL levels for security logs
            .all_modules()
            .span_pattern("security*"), // Only security spans
    );

    // Create config with all tabs
    let config = TracerConfig::from_tabs([
        ("stdout", stdout_matcher),
        ("database", db_matcher),
        ("network", network_matcher),
        ("security", security_matcher),
    ]);

    // Initialize the tracer with the config
    let tracer = Tracer::init(config)?;

    // Set up channels for logger commands
    let (tx, mut rx) = mpsc::unbounded_channel::<LogMessage>();

    // Initialize the destination logger
    let logger = Arc::new(Mutex::new(DestinationLogger::new(log_dir.clone())?));

    // Process log commands in background
    let logger_clone = Arc::clone(&logger);
    tokio::spawn(async move {
        while let Some(LogMessage {
            destination,
            message,
        }) = rx.recv().await
        {
            let mut logger = logger_clone.lock().unwrap();
            if let Err(e) = logger.log_message(&destination, message) {
                eprintln!("Error logging to {destination}: {e}");
            }
        }
    });
    // Set callback for captured events
    let tx_clone = tx.clone();
    tracer
        .set_callback(move |event, tab_names| {
            for &dest in tab_names {
                let _ = tx_clone.send(LogMessage {
                    destination: dest.to_string(),
                    message: Arc::clone(&event),
                });
            }
        })?
        .await??;

    // Log dropped events to stdout for debugging
    let _tx_clone = tx.clone();
    tracer
        .set_dropped_callback(move |event| {
            println!("DROPPED: {}", event.format());
        })?
        .await??;

    // Generate example logs with different spans to demonstrate routing
    tokio::spawn(async move {
        // General logs without spans - should go to stdout
        for i in 0..5 {
            info!("General application info log {}", i);
            warn!("General application warning {}", i);

            if i % 2 == 0 {
                error!("General application error {}", i);
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Database span logs - should go to database file
        {
            let db_span = span!(Level::INFO, "database_operations");
            let _guard = db_span.enter();

            for i in 0..3 {
                debug!("Database connection pool status check {}", i);
                info!("Database query executed successfully {}", i);

                if i == 1 {
                    warn!("Database query performance warning {}", i);
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }

        // Network span logs - should go to network file
        {
            let net_span = span!(Level::INFO, "network_requests");
            let _guard = net_span.enter();

            for i in 0..4 {
                info!("Network request processed {}", i);
                debug!("Network connection details: port={}", 8000 + i);

                if i == 2 {
                    error!("Network connection timeout error!");
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }

        // Security span logs - should go to security file, including trace level
        {
            let sec_span = span!(Level::INFO, "security_audit");
            let _guard = sec_span.enter();

            for i in 0..3 {
                trace!("Security trace: user session activity {}", i);
                debug!("Security check performed for user {}", 1000 + i);
                info!("Security policy applied {}", i);

                if i == 1 {
                    warn!("Potential security issue detected: multiple failed logins");
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }

        // Nested spans for more complex routing
        {
            let app_span = span!(Level::INFO, "application");
            let _app_guard = app_span.enter();

            info!("Application started with main configuration");

            // This should go to database file
            {
                let db_span = span!(Level::INFO, "database_init");
                let _db_guard = db_span.enter();

                info!("Database initialized");
                debug!("Connection pool created with 10 connections");
            }

            // This should go to network file
            {
                let net_span = span!(Level::INFO, "network_init");
                let _net_guard = net_span.enter();

                info!("Network listeners started");
                debug!("Listening on ports 8080, 8081");
            }

            info!("Application initialization complete");
        }

        // Print statistics after all logs are generated
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let logger = logger.lock().unwrap();
        logger.print_stats();

        let captured = tracer.get_captured_count();
        let silenced = tracer.get_silenced_count();
        let dropped = tracer.get_dropped_count();

        println!("Tracer statistics:");
        println!("  Captured: {captured}");
        println!("  Silenced: {silenced}");
        println!("  Dropped: {dropped}");

        println!("\nLog files are in: {}", log_dir.display());
        println!("Example completed - press Ctrl+C to exit");
    });

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    println!("Shutting down...");

    Ok(())
}
