// examples/multiple-spans.rs
use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use tokio_tracer::{Matcher, MatcherSet, Tracer, TracerConfig};
use tracing::{Level, debug, error, info, span, trace, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Create a log file
    let log_path = std::env::temp_dir().join("span_matcher_demo.log");
    println!("Log file path: {}", log_path.display());

    let log_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)?;

    let log_file = std::sync::Mutex::new(log_file);

    // Console filter - logs that are NOT in the "database" span
    let console_matcher = MatcherSet::from_matchers([
        // Include normal logs
        Matcher::info().all_modules(),
        // Exclude database spans
        Matcher::trace().exclude().span_pattern("database*"),
        // Exclude database spans
        Matcher::trace().exclude().target_pattern("foobar"),
    ]);

    // File filter - only logs in the "database" span
    let file_matcher = MatcherSet::from_matchers([
        Matcher::trace().span_pattern("database*"),
        Matcher::trace().target_pattern("fooba*"),
    ]);

    // Initialize the tracer
    let config =
        TracerConfig::from_tabs([("console", console_matcher), ("file-only", file_matcher)]);

    let tracer = Tracer::init(config)?;

    // Regular log - goes to console
    info!("Starting application");
    info!("okay here we go");

    // Set callbacks
    tracer
        .set_callback(move |event, tab_names| {
            for &target in tab_names {
                match target {
                    "file-only" => {
                        let mut file = log_file.lock().unwrap();
                        writeln!(file, "FILE: {}", event.format_full()).unwrap();
                    }
                    "console" => {
                        println!("{}", event.format());
                    }
                    _ => unreachable!(),
                }
            }
        })?
        .await??;
    info!("okay here we go again");

    warn!("This is a general warning");

    info!(target:"foobar","THIS IS A SPECIAL TARGET MESSAGE");

    // Database span logs - go to file only
    {
        let db_span = span!(Level::INFO, "database_query");
        let _guard = db_span.enter();

        trace!("Database connection established");
        debug!("Preparing SQL query");
        info!("Executing query: SELECT * FROM users");
        warn!("Query took longer than expected: 250ms");
        error!("Query error: deadlock detected");
    }

    // Regular logs after span - go to console
    info!("Continuing with application logic");

    // Nested span with database child - parent goes to console, child to file
    {
        let app_span = span!(Level::INFO, "application");
        let _app_guard = app_span.enter();

        info!("This log is in the application span"); // Console

        {
            let db_span = span!(Level::INFO, "database_connection");
            let _db_guard = db_span.enter();

            info!("This log is in the database span"); // File
            debug!("Database connection details"); // File
        }

        info!("Back to application span"); // Console
    }

    // Display stats
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    println!("\nCapture stats:");
    println!("Captured events: {}", tracer.get_captured_count());
    println!("Silenced events: {}", tracer.get_silenced_count());
    println!("Dropped events: {}", tracer.get_dropped_count());

    println!("\nCheck the log file at: {}", log_path.display());
    println!("Log file contents:");

    // Print the file contents for verification
    let file_content = std::fs::read_to_string(&log_path)?;
    println!("{file_content}");

    Ok(())
}
