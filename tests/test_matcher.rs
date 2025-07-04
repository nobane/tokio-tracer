// tests/test_matcher.rs

use chrono::Local;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_tracer::{Matcher, MatcherSet, TraceData, TraceLevel, matches};
use tracing::Level;

// Helper to create a test trace event with specific properties
#[allow(clippy::too_many_arguments)]
fn create_test_event(
    id: u64,
    level: Level,
    event: &str,
    module_path: Option<&str>,
    file: Option<&str>,
    line: Option<u32>,
    span_name: Option<&str>,
    fields: Option<HashMap<String, String>>,
) -> TraceData {
    TraceData {
        id,
        timestamp: Local::now(),
        level: TraceLevel(level),
        target: "test_target".to_string(),
        name: "test_event".to_string(),
        module_path: module_path.map(|s| s.to_string()),
        file: file.map(|s| s.to_string()),
        line,
        message: event.to_string(),
        fields: fields.unwrap_or_default(),
        span_name: span_name.map(|s| s.to_string()),
        span_hierarchy: span_name.map(|s| s.to_string()), // Initialize with the same value as span_name
    }
}

#[test]
fn test_trace_event_creation() {
    let id = 1;
    let level = Level::INFO;
    let message = "Test event";
    let module_path = Some("test_module");
    let file = Some("test_file.rs");
    let line = Some(42);
    let span_name = Some("test_span");

    let mut fields = HashMap::new();
    fields.insert("test_key".to_string(), "test_value".to_string());
    fields.insert("number".to_string(), "123".to_string());

    let event = create_test_event(
        id,
        level,
        message,
        module_path,
        file,
        line,
        span_name,
        Some(fields.clone()),
    );

    // Verify basic properties
    assert_eq!(event.id, id);
    assert_eq!(event.level, TraceLevel(level));
    assert_eq!(event.message, message);
    assert_eq!(event.module_path, module_path.map(|s| s.to_string()));
    assert_eq!(event.file, file.map(|s| s.to_string()));
    assert_eq!(event.line, line);
    assert_eq!(event.span_name, span_name.map(|s| s.to_string()));
    assert_eq!(event.span_hierarchy, span_name.map(|s| s.to_string()));

    // Verify fields
    assert_eq!(event.fields.len(), 2);
    assert_eq!(event.fields.get("test_key").unwrap(), "test_value");
    assert_eq!(event.fields.get("number").unwrap(), "123");
}

#[test]
fn test_trace_event_formatting() {
    let event = create_test_event(
        1,
        Level::WARN,
        "Warning event",
        Some("test_module"),
        Some("test_file.rs"),
        Some(42),
        Some("test_span"),
        None,
    );

    // Test basic format
    let formatted = event.format();
    assert!(formatted.contains("WARN"));
    assert!(formatted.contains("Warning event"));
    assert!(formatted.contains("[test_module]"));
    assert!(formatted.contains("[span:test_span]"));

    // Test format with file
    let formatted_with_file = event.format_with_file();
    assert!(formatted_with_file.contains("test_file.rs:42"));

    // Test with fields
    let mut fields = HashMap::new();
    fields.insert("key1".to_string(), "value1".to_string());
    fields.insert("key2".to_string(), "value2".to_string());

    let event_with_fields = create_test_event(
        2,
        Level::ERROR,
        "Error event",
        Some("error_module"),
        Some("error.rs"),
        Some(100),
        None,
        Some(fields),
    );

    let formatted_with_fields = event_with_fields.format_with_fields();
    assert!(formatted_with_fields.contains("key1=value1"));
    assert!(formatted_with_fields.contains("key2=value2"));
}

#[test]
fn test_trace_event_display() {
    let event = create_test_event(
        1,
        Level::INFO,
        "Display test",
        Some("display_module"),
        Some("display.rs"),
        Some(55),
        None,
        None,
    );

    let display_string = format!("{event}");
    assert!(display_string.contains("INFO"));
    assert!(display_string.contains("Display test"));
    assert!(display_string.contains("[display_module]"));
}

#[test]
fn test_trace_event_arc_methods() {
    let event = create_test_event(1, Level::DEBUG, "Arc test", None, None, None, None, None);
    let arc_event = Arc::new(event);

    // Test ref_count
    assert_eq!(arc_event.ref_count(), 1);

    // Create another reference
    let arc_event2 = arc_event.clone();
    assert_eq!(arc_event.ref_count(), 2);

    // Test ptr_eq
    assert!(arc_event.ptr_eq(&arc_event2));

    // Test with different event
    let different_event = Arc::new(create_test_event(
        2,
        Level::TRACE,
        "Different event",
        None,
        None,
        None,
        None,
        None,
    ));
    assert!(!arc_event.ptr_eq(&different_event));
}

#[test]
fn test_pattern_matching() {
    // Test exact matches
    assert!(matches("exact", "exact"));
    assert!(!matches("exact", "not_exact"));

    // Test wildcard at end
    assert!(matches("prefix*", "prefix"));
    assert!(matches("prefix*", "prefix_with_more"));
    assert!(!matches("prefix*", "wrong_prefix"));

    // Test wildcard at beginning
    assert!(matches("*suffix", "suffix"));
    assert!(matches("*suffix", "with_suffix"));
    assert!(!matches("*suffix", "wrong_suffix_extra"));

    // Test wildcard in middle
    assert!(matches("pre*post", "prepost"));
    assert!(matches("pre*post", "pre_middle_post"));
    assert!(!matches("pre*post", "pre_wrong"));
    assert!(!matches("pre*post", "wrong_post"));

    // Test multiple wildcards
    assert!(matches("*mid*", "mid"));
    assert!(matches("*mid*", "prefix_mid"));
    assert!(matches("*mid*", "mid_suffix"));
    assert!(matches("*mid*", "prefix_mid_suffix"));
    assert!(!matches("*mid*", "nomatch"));
}

#[test]
fn test_trace_level() {
    // Test creation and conversion
    let trace_level = TraceLevel(Level::INFO);
    assert_eq!(trace_level.0, Level::INFO);

    // Test display
    assert_eq!(format!("{trace_level}"), "INFO");

    // Test from/into
    let filter: Level = trace_level.into();
    assert_eq!(filter, Level::INFO);

    let level_from_matcher = TraceLevel::from(Level::DEBUG);
    assert_eq!(level_from_matcher.0, Level::DEBUG);

    // Test equality
    assert_eq!(TraceLevel(Level::WARN), TraceLevel(Level::WARN));
    assert_ne!(TraceLevel(Level::ERROR), TraceLevel(Level::INFO));
}

#[test]
fn test_trace_matcher() {
    // Create test events with different levels and modules
    let info_event = create_test_event(
        1,
        Level::INFO,
        "Info event",
        Some("module_a"),
        Some("file_a.rs"),
        Some(10),
        None,
        None,
    );

    let debug_event = create_test_event(
        2,
        Level::DEBUG,
        "Debug event",
        Some("module_a"),
        Some("file_a.rs"),
        Some(20),
        None,
        None,
    );

    let error_event = create_test_event(
        3,
        Level::ERROR,
        "Error event",
        Some("module_b"),
        Some("file_b.rs"),
        Some(30),
        None,
        None,
    );

    let no_module_event = create_test_event(
        4,
        Level::WARN,
        "No module",
        None,
        Some("unknown.rs"),
        Some(40),
        None,
        None,
    );

    let span_event = create_test_event(
        5,
        Level::INFO,
        "Span event",
        Some("module_a"),
        Some("file_a.rs"),
        Some(50),
        Some("test_span"),
        None,
    );

    // Test level filtering - using builder pattern
    let level_matcher = Matcher::info().all_modules();

    assert!(level_matcher.matches(&info_event)); // INFO passes INFO filter
    assert!(!level_matcher.matches(&debug_event)); // DEBUG doesn't pass INFO filter
    assert!(level_matcher.matches(&error_event)); // ERROR passes INFO filter
    assert!(level_matcher.matches(&no_module_event)); // WARN passes INFO filter

    // Test module pattern filtering
    let module_matcher = Matcher::trace().module_pattern("module_a*");

    assert!(module_matcher.matches(&info_event)); // module_a matches
    assert!(module_matcher.matches(&debug_event)); // module_a matches
    assert!(!module_matcher.matches(&error_event)); // module_b doesn't match
    assert!(!module_matcher.matches(&no_module_event)); // no module doesn't match

    // Test file pattern filtering
    let file_matcher = Matcher::trace().file_pattern("file_b*");

    assert!(!file_matcher.matches(&info_event)); // file_a doesn't match
    assert!(!file_matcher.matches(&debug_event)); // file_a doesn't match
    assert!(file_matcher.matches(&error_event)); // file_b matches
    assert!(!file_matcher.matches(&no_module_event)); // unknown doesn't match

    // Test combined filtering
    let combined_matcher = Matcher::info()
        .module_pattern("module_a*")
        .file_pattern("file_a*");

    assert!(combined_matcher.matches(&info_event)); // matches all criteria
    assert!(!combined_matcher.matches(&debug_event)); // fails level check
    assert!(!combined_matcher.matches(&error_event)); // fails module and file check
    assert!(!combined_matcher.matches(&no_module_event)); // fails all checks

    // Test span pattern filtering
    let span_matcher = Matcher::info().all_modules().span_pattern("test_*");

    assert!(!span_matcher.matches(&info_event)); // no span doesn't match
    assert!(span_matcher.matches(&span_event)); // test_span matches
}

#[test]
fn test_target_pattern_matchering() {
    // Create test events with different targets
    let mut custom_fields = HashMap::new();
    custom_fields.insert("key".to_string(), "value".to_string());

    let event1 = create_test_event(
        1,
        Level::INFO,
        "Standard target",
        Some("module_a"),
        Some("file_a.rs"),
        Some(10),
        None,
        Some(custom_fields.clone()),
    );

    let mut event2 = create_test_event(
        2,
        Level::INFO,
        "Custom target",
        Some("module_a"),
        Some("file_a.rs"),
        Some(10),
        None,
        Some(custom_fields.clone()),
    );
    event2.target = "custom_target".to_string();

    let mut event3 = create_test_event(
        3,
        Level::INFO,
        "API target",
        Some("module_a"),
        Some("file_a.rs"),
        Some(10),
        None,
        Some(custom_fields.clone()),
    );
    event3.target = "api_service".to_string();

    // Test target pattern filtering
    let target_matcher = Matcher::info().all_modules().target_pattern("custom*");

    assert!(!target_matcher.matches(&event1)); // test_target doesn't match custom*
    assert!(target_matcher.matches(&event2)); // custom_target matches custom*
    assert!(!target_matcher.matches(&event3)); // api_service doesn't match custom*

    // Test combined target and module filtering
    let combined_matcher = Matcher::info()
        .module_pattern("module_a*")
        .target_pattern("api*");

    assert!(!combined_matcher.matches(&event1)); // target doesn't match
    assert!(!combined_matcher.matches(&event2)); // target doesn't match
    assert!(combined_matcher.matches(&event3)); // both module and target match

    // Test multiple target patterns
    let multi_target_matcher = Matcher::info()
        .all_modules()
        .target_patterns(vec!["custom*", "api*"]);

    assert!(!multi_target_matcher.matches(&event1)); // doesn't match any target pattern
    assert!(multi_target_matcher.matches(&event2)); // matches custom*
    assert!(multi_target_matcher.matches(&event3)); // matches api*

    // Test with wildcard target pattern
    let wildcard_target_matcher = Matcher::info().all_modules();

    assert!(wildcard_target_matcher.matches(&event1)); // matches wildcard
    assert!(wildcard_target_matcher.matches(&event2)); // matches wildcard
    assert!(wildcard_target_matcher.matches(&event3)); // matches wildcard
}

#[test]
fn test_matcher_comparison() {
    // Create two identical filters using builder pattern
    let filter1 = Matcher::info()
        .module_patterns(vec!["pattern1", "pattern2"])
        .file_pattern("file_pattern")
        .span_pattern("span_pattern")
        .target_pattern("target_pattern");

    let filter2 = Matcher::info()
        .module_patterns(vec!["pattern1", "pattern2"])
        .file_pattern("file_pattern")
        .span_pattern("span_pattern")
        .target_pattern("target_pattern");

    // They should be equal
    assert_eq!(filter1, filter2);

    // Change level
    let filter3 = Matcher::debug()
        .module_patterns(vec!["pattern1", "pattern2"])
        .file_pattern("file_pattern")
        .span_pattern("span_pattern")
        .target_pattern("target_pattern");
    assert_ne!(filter1, filter3);

    // Change module patterns
    let filter4 = Matcher::info()
        .module_pattern("different")
        .file_pattern("file_pattern")
        .span_pattern("span_pattern")
        .target_pattern("target_pattern");
    assert_ne!(filter1, filter4);

    // Change file patterns
    let filter5 = Matcher::info()
        .module_patterns(vec!["pattern1", "pattern2"])
        .file_pattern("different")
        .span_pattern("span_pattern")
        .target_pattern("target_pattern");
    assert_ne!(filter1, filter5);

    // Change span patterns
    let filter6 = Matcher::info()
        .module_patterns(vec!["pattern1", "pattern2"])
        .file_pattern("file_pattern")
        .span_pattern("different_span")
        .target_pattern("target_pattern");
    assert_ne!(filter1, filter6);

    // Change target patterns
    let filter7 = Matcher::info()
        .module_patterns(vec!["pattern1", "pattern2"])
        .file_pattern("file_pattern")
        .span_pattern("span_pattern")
        .target_pattern("different_target");
    assert_ne!(filter1, filter7);
}
#[test]
fn test_span_based_matchering() {
    // Create events with different spans
    let no_span = create_test_event(
        1,
        Level::INFO,
        "No span",
        Some("module_a"),
        Some("file.rs"),
        Some(10),
        None,
        None,
    );

    let database_span = create_test_event(
        2,
        Level::INFO,
        "Database span",
        Some("module_a"),
        Some("file.rs"),
        Some(20),
        Some("database_query"),
        None,
    );

    let network_span = create_test_event(
        3,
        Level::INFO,
        "Network span",
        Some("module_a"),
        Some("file.rs"),
        Some(30),
        Some("network_request"),
        None,
    );

    // Create filter that only matches database spans using builder
    let db_matcher = Matcher::info().all_modules().span_pattern("database_*");

    assert!(!db_matcher.matches(&no_span)); // no span doesn't match
    assert!(db_matcher.matches(&database_span)); // database span matches
    assert!(!db_matcher.matches(&network_span)); // network span doesn't match

    // Create filter that excludes network spans but includes everything else
    let filter_set = MatcherSet::empty()
        .with_matcher(Matcher::info().all_modules())
        .with_matcher(Matcher::info().exclude().span_pattern("network_*"));

    // Get the matchers and identify them by their properties instead of assuming order
    let matchers = filter_set.iter_matchers();

    // Find the include matcher (should have include=true and no span patterns)
    let include_matcher = matchers
        .iter()
        .find(|m| m.include && m.span_patterns.is_empty())
        .unwrap();

    // Find the exclude matcher (should have include=false and network_* span pattern)
    let exclude_matcher = matchers
        .iter()
        .find(|m| !m.include && m.span_patterns.contains(&"network_*".to_string()))
        .unwrap();

    // Test the include matcher (should match all events)
    assert!(include_matcher.matches(&no_span)); // include matcher matches no_span
    assert!(include_matcher.matches(&database_span)); // include matcher matches database_span
    assert!(include_matcher.matches(&network_span)); // include matcher matches network_span

    // Test the exclude matcher (should only match events that have network spans)
    // For exclude filters, .matches() returns true when the event SHOULD BE EXCLUDED
    assert!(!exclude_matcher.matches(&no_span)); // exclude matcher doesn't match no_span (no span to exclude)
    assert!(!exclude_matcher.matches(&database_span)); // exclude matcher doesn't match database_span (not network)
    assert!(exclude_matcher.matches(&network_span)); // exclude matcher matches network_span (should be excluded)

    // Test the actual filtering logic that would be used in the dispatcher
    // An event is captured if any include filter matches AND no exclude filter matches

    // no_span: included by include matcher, not excluded by exclude matcher -> CAPTURED
    let no_span_included = include_matcher.matches(&no_span);
    let no_span_excluded = exclude_matcher.matches(&no_span);
    assert!(no_span_included && !no_span_excluded); // should be captured

    // database_span: included by include matcher, not excluded by exclude matcher -> CAPTURED
    let db_span_included = include_matcher.matches(&database_span);
    let db_span_excluded = exclude_matcher.matches(&database_span);
    assert!(db_span_included && !db_span_excluded); // should be captured

    // network_span: included by include matcher, but excluded by exclude matcher -> SILENCED
    let network_span_included = include_matcher.matches(&network_span);
    let network_span_excluded = exclude_matcher.matches(&network_span);
    assert!(network_span_included && network_span_excluded); // should be silenced
}

#[test]
fn test_builder_pattern() {
    // Test creating filters with the new builder pattern

    // Create an ERROR level filter that excludes specific modules
    let error_matcher = Matcher::error()
        .exclude()
        .module_pattern("internal*")
        .module_pattern("test*");

    assert_eq!(error_matcher.level, TraceLevel(Level::ERROR));
    assert!(!error_matcher.include);
    assert_eq!(error_matcher.module_patterns.len(), 2);
    assert!(
        error_matcher
            .module_patterns
            .contains(&"internal*".to_string())
    );
    assert!(error_matcher.module_patterns.contains(&"test*".to_string()));

    // Create an INFO level filter with file, span and target patterns
    let complex_matcher = Matcher::info()
        .module_pattern("app*")
        .file_pattern("src/*.rs")
        .span_pattern("database*")
        .target_pattern("api*");

    assert_eq!(complex_matcher.level, TraceLevel(Level::INFO));
    assert!(complex_matcher.include);
    assert_eq!(complex_matcher.module_patterns.len(), 1);
    assert_eq!(complex_matcher.file_patterns.len(), 1);
    assert_eq!(complex_matcher.span_patterns.len(), 1);
    assert_eq!(complex_matcher.target_patterns.len(), 1);

    // Test the "all" shortcuts
    let all_matcher = Matcher::debug().all_modules();

    assert_eq!(all_matcher.level, TraceLevel(Level::DEBUG));
    assert!(all_matcher.module_patterns.contains(&"*".to_string()));

    // Test replacing patterns
    let replaced_matcher = Matcher::warn()
        .module_pattern("old_pattern")
        .module_patterns(vec!["new_pattern1", "new_pattern2"]);

    assert_eq!(replaced_matcher.module_patterns.len(), 2);
    assert!(
        !replaced_matcher
            .module_patterns
            .contains(&"old_pattern".to_string())
    );
    assert!(
        replaced_matcher
            .module_patterns
            .contains(&"new_pattern1".to_string())
    );
    assert!(
        replaced_matcher
            .module_patterns
            .contains(&"new_pattern2".to_string())
    );
}
