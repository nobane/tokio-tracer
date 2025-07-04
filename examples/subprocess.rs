// examples/subprocess.rs
use anyhow::Result;
use std::env;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tracer::{Matcher, MatcherSet, Tracer, TracerConfig};
use tracing::{debug, error, info, trace, warn};

/// Run application as child process, writing logs directly to stdout
async fn run_as_child() -> Result<()> {
    // Create filter set for all levels
    let filter_set = MatcherSet::from_matcher(Matcher::trace().all_modules());

    // Create config with the child tab
    let config = TracerConfig::from_tab(("Child", filter_set));

    // Initialize tracer with the config
    let tracer = Tracer::init(config)?;

    // Set callback to print formatted logs to stdout
    tracer
        .set_callback(move |event, _tab_names| {
            println!("C|{}", event.format());
        })?
        .await??;

    // Generate sample logs with different levels
    for i in 0..10 {
        trace!("Child trace message {}", i);
        debug!("Child debug message {}", i);
        info!("Child info message {}", i);

        if i % 3 == 0 {
            warn!("Child warning message {}", i);
        }

        if i % 5 == 0 {
            error!("Child error message {}", i);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    Ok(())
}

/// Parent process that spawns child and handles both sets of logs
async fn run_as_parent() -> Result<()> {
    // Create filter set for parent logs
    let filter_set = MatcherSet::from_matcher(Matcher::trace().all_modules());

    // Create config with the parent tab
    let config = TracerConfig::from_tab(("Parent", filter_set));

    // Initialize tracer with the config
    let tracer = Tracer::init(config)?;

    // Shared counter to track logs from both sources
    let log_count = Arc::new(Mutex::new((0, 0))); // (parent_count, child_count)
    let log_count_clone = log_count.clone();

    // Set callback to print parent logs with "P" prefix
    tracer
        .set_callback(move |event, _tab_names| {
            let count_clone = log_count_clone.clone();

            tokio::spawn(async move {
                let mut counts = count_clone.lock().await;
                counts.0 += 1; // Increment parent log count
                println!("P|{}", event.format());
            });
        })?
        .await??;

    // Spawn child process with --child flag
    let child = Command::new(env::current_exe()?)
        .arg("--child")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    info!("Parent started child process with PID: {}", child.id());

    // Create thread to read from child's stdout
    let stdout = child.stdout.expect("Failed to capture child stdout");
    let stderr = child.stderr.expect("Failed to capture child stderr");
    let log_count_clone = log_count.clone();

    // Handle child stdout
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    let mut counts = log_count_clone.lock().await;
                    counts.1 += 1; // Increment child log count
                    println!("{line}"); // Already has "C|" prefix from child
                }
                Err(e) => eprintln!("Error reading from child stdout: {e}"),
            }
        }
    });

    // Handle child stderr
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    eprintln!("Child stderr: {line}");
                }
                Err(e) => eprintln!("Error reading from child stderr: {e}"),
            }
        }
    });

    // Generate sample logs from the parent
    for i in 0..15 {
        trace!("Parent trace message {}", i);
        debug!("Parent debug message {}", i);
        info!("Parent info message {}", i);

        if i % 4 == 0 {
            warn!("Parent warning message {}", i);
        }

        if i % 7 == 0 {
            error!("Parent error message {}", i);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(700)).await;
    }

    // Print stats after all logs are generated
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let counts = log_count.lock().await;
    info!("Log statistics - Parent: {}, Child: {}", counts.0, counts.1);
    info!("Total captured logs: {}", tracer.get_captured_count());

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Check if we're running as a child process
    let args: Vec<String> = env::args().collect();
    let is_child = args.len() > 1 && args[1] == "--child";

    if is_child {
        println!("Starting as child process");
        run_as_child().await?;
    } else {
        println!("Starting as parent process");
        run_as_parent().await?;
    }

    Ok(())
}
