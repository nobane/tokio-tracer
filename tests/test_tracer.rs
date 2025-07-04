// tests/test_tracer.rs
#[cfg(test)]
mod test_tracer {
    use anyhow::Result;
    use std::{collections::HashMap, sync::Arc, time::Duration};
    use tokio::sync::Mutex;
    use tokio_tracer::{
        Matcher, MatcherSet, TraceData, TraceEvent, TraceLevel, Tracer, TracerConfig, TracerTab,
    };
    use tracing::Level;

    // Helper to create a test trace event
    fn create_test_event(
        id: u64,
        level: Level,
        event: &str,
        module_path: Option<&str>,
        file: Option<&str>,
        line: Option<u32>,
        span_name: Option<&str>,
    ) -> TraceEvent {
        let mut event = TraceData {
            id,
            timestamp: chrono::Local::now(),
            level: TraceLevel(level),
            target: "test_target".to_string(),
            name: "test_event".to_string(),
            module_path: module_path.map(|s| s.to_string()),
            file: file.map(|s| s.to_string()),
            line,
            message: event.to_string(),
            fields: HashMap::new(),
            span_name: span_name.map(|s| s.to_string()),
            span_hierarchy: span_name.map(|s| s.to_string()), // Initialize with same value as span_name
        };

        // Add some test fields
        event
            .fields
            .insert("test_field".to_string(), "test_value".to_string());

        Arc::new(event)
    }

    // Helper to create a test trace event with custom target
    #[allow(clippy::too_many_arguments)]
    fn create_test_event_with_target(
        id: u64,
        level: Level,
        event: &str,
        module_path: Option<&str>,
        target: &str,
        file: Option<&str>,
        line: Option<u32>,
        span_name: Option<&str>,
    ) -> TraceEvent {
        let mut event = TraceData {
            id,
            timestamp: chrono::Local::now(),
            level: TraceLevel(level),
            target: target.to_string(),
            name: "test_event".to_string(),
            module_path: module_path.map(|s| s.to_string()),
            file: file.map(|s| s.to_string()),
            line,
            message: event.to_string(),
            fields: HashMap::new(),
            span_name: span_name.map(|s| s.to_string()),
            span_hierarchy: span_name.map(|s| s.to_string()),
        };

        // Add some test fields
        event
            .fields
            .insert("test_field".to_string(), "test_value".to_string());

        Arc::new(event)
    }

    // Helper method to send a test event to the tracer
    async fn send_event(tracer: &Tracer, event: TraceEvent) {
        // Access the internal event_tx channel
        let _ = tracer._get_sender_for_testing().send(event);

        // Give the event time to be processed
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    #[tokio::test]
    async fn test_error_handling() -> Result<()> {
        // Initialize tracer
        let tracer = Tracer::new_with_config(TracerConfig::default_main_tab());

        // Test removing non-existent tab
        let remove_result = tracer.remove_tab("nonexistent")?.await?;
        assert!(
            remove_result.is_err(),
            "Removing non-existent tab should return error"
        );

        if let Err(e) = remove_result {
            assert!(
                e.to_string().contains("not found"),
                "Error event should mention tab not found"
            );
        }

        // Test updating non-existent tab
        let update_result = tracer
            .update_tab("nonexistent", MatcherSet::empty())?
            .await?;

        assert!(
            update_result.is_err(),
            "Updating non-existent tab should return error"
        );

        if let Err(e) = update_result {
            assert!(
                e.to_string().contains("not found"),
                "Error event should mention tab not found"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_tab_management() -> Result<()> {
        // Create filter sets using builder pattern
        let mut matcher_set1 = MatcherSet::empty();
        matcher_set1.add_matcher(Matcher::info().module_pattern("module_a*"));

        let mut matcher_set2 = MatcherSet::empty();
        matcher_set2.add_matcher(Matcher::debug().module_pattern("module_b*"));

        // Create a config with initial tabs
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("tab_a".to_string()).with_matcher_set(matcher_set1.clone()),
                TracerTab::new("tab_b".to_string()).with_matcher_set(matcher_set2.clone()),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Use more detailed tracking for debugging
        let tab_a_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let tab_b_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let sub_a_clone = tab_a_messages.clone();
        let sub_b_clone = tab_b_messages.clone();

        // Set callback
        tracer
            .set_callback(move |event, tabs| {
                println!("Callback: {} - {:?}", event.message, tabs);

                // Check which tabs received this event
                for &tab in tabs {
                    let msg_text = event.message.clone();

                    if tab.eq("tab_a") {
                        let sub_a = sub_a_clone.clone();
                        tokio::spawn(async move {
                            let mut lock = sub_a.lock().await;
                            lock.push(msg_text.clone());
                        });
                    } else if tab.eq("tab_b") {
                        let sub_b = sub_b_clone.clone();
                        tokio::spawn(async move {
                            let mut lock = sub_b.lock().await;
                            lock.push(msg_text.clone());
                        });
                    }
                }
            })?
            .await??;

        // Create test events for each tab
        let event_a = create_test_event(
            1,
            Level::INFO,
            "Module A event",
            Some("module_a"),
            Some("test.rs"),
            Some(42),
            None,
        );

        let event_b = create_test_event(
            2,
            Level::DEBUG,
            "Module B event",
            Some("module_b"),
            Some("test.rs"),
            Some(43),
            None,
        );

        // Send events directly to the tracer
        let event_tx = tracer._get_sender_for_testing();
        let _ = event_tx.send(event_a.clone());
        let _ = event_tx.send(event_b.clone());

        // Give time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check each tab's event count
        {
            let sub_a_messages = tab_a_messages.lock().await;
            let sub_b_messages = tab_b_messages.lock().await;

            println!("A messages: {:?}", *sub_a_messages);
            println!("B messages: {:?}", *sub_b_messages);

            assert_eq!(
                sub_a_messages.len(),
                1,
                "Subscriber A should have received 1 event"
            );
            assert_eq!(
                sub_b_messages.len(),
                1,
                "Subscriber B should have received 1 event"
            );
        }

        // Completely clear tabs and add them back to ensure clean state
        tracer.remove_tab("tab_a")?.await??;
        tracer.remove_tab("tab_b")?.await??;

        // Clear stats to start fresh
        tracer.clear_stats()?.await??;

        // Add tabs again
        tracer.add_tab("tab_a", matcher_set1.clone())?.await??;
        tracer.add_tab("tab_b", matcher_set2.clone())?.await??;

        // Update tab_a to include module_b messages
        let mut updated_matcher_set = matcher_set1.clone();
        updated_matcher_set.add_matcher(Matcher::debug().module_pattern("module_b*"));

        tracer.update_tab("tab_a", updated_matcher_set)?.await??;

        // Reset event tracking
        let tab_a_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let tab_b_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let sub_a_clone = tab_a_messages.clone();
        let sub_b_clone = tab_b_messages.clone();

        // Update callback
        tracer
            .set_callback(move |event, tabs| {
                println!("Callback 2: {} - {:?}", event.message, tabs);

                // Check which tabs received this event
                for &tab in tabs {
                    let msg_text = event.message.clone();

                    if tab.eq("tab_a") {
                        let sub_a = sub_a_clone.clone();
                        tokio::spawn(async move {
                            let mut lock = sub_a.lock().await;
                            lock.push(msg_text.clone());
                        });
                    } else if tab.eq("tab_b") {
                        let sub_b = sub_b_clone.clone();
                        tokio::spawn(async move {
                            let mut lock = sub_b.lock().await;
                            lock.push(msg_text.clone());
                        });
                    }
                }
            })?
            .await??;

        // Create another module_b event
        let event_b2 = create_test_event(
            3,
            Level::DEBUG,
            "Another Module B event",
            Some("module_b"),
            Some("test.rs"),
            Some(44),
            None,
        );

        // Send the event
        let _ = tracer._get_sender_for_testing().send(event_b2.clone());

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check that tab_a now gets module_b messages too
        {
            let sub_a_messages = tab_a_messages.lock().await;
            let sub_b_messages = tab_b_messages.lock().await;

            println!("After update - A messages: {:?}", *sub_a_messages);
            println!("After update - B messages: {:?}", *sub_b_messages);

            assert_eq!(
                sub_a_messages.len(),
                1,
                "Subscriber A should have received 1 event after update"
            );
            assert_eq!(
                sub_b_messages.len(),
                1,
                "Subscriber B should have received 1 event"
            );
        }

        // Remove tab_b
        tracer.remove_tab("tab_b")?.await??;

        // Create another module_b event
        let event_b3 = create_test_event(
            4,
            Level::DEBUG,
            "Third Module B event",
            Some("module_b"),
            Some("test.rs"),
            Some(45),
            None,
        );

        // Send the event
        let _ = tracer._get_sender_for_testing().send(event_b3.clone());

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check that only tab_a gets the event
        {
            let sub_a_messages = tab_a_messages.lock().await;
            let sub_b_messages = tab_b_messages.lock().await;

            println!("After removal - A messages: {:?}", *sub_a_messages);
            println!("After removal - B messages: {:?}", *sub_b_messages);

            assert_eq!(
                sub_a_messages.len(),
                2,
                "Subscriber A should have received 2 messages"
            );
            assert_eq!(
                sub_b_messages.len(),
                1,
                "Subscriber B should still have 1 event after removal"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_callback_functionality() -> Result<()> {
        // Create two filter sets for different tabs using builder pattern
        let mut matcher_set1 = MatcherSet::empty();
        matcher_set1.add_matcher(Matcher::info().module_pattern("module_a*"));

        let mut matcher_set2 = MatcherSet::empty();
        matcher_set2.add_matcher(
            Matcher::error() // Higher level filter
                .module_pattern("module_a*"), // Same module
        );

        // Create a config with initial tabs
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("lax_tab".to_string()).with_matcher_set(matcher_set1),
                TracerTab::new("strict_tab".to_string()).with_matcher_set(matcher_set2),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Track callback invocations
        #[derive(Debug)]
        struct CallbackData {
            event: String,
            tabs: Vec<String>,
        }

        let callback_data = Arc::new(Mutex::new(Vec::<CallbackData>::new()));
        let data_clone = callback_data.clone();

        // Set callback
        tracer
            .set_callback(move |event, tabs| {
                let data = data_clone.clone();
                let msg = event.message.clone();
                let sub_list: Vec<String> = tabs.iter().map(|&s| s.to_string()).collect();

                tokio::spawn(async move {
                    let mut lock = data.lock().await;
                    lock.push(CallbackData {
                        event: msg,
                        tabs: sub_list,
                    });
                });
            })?
            .await??;

        // Create test events with different levels
        let info_event = create_test_event(
            1,
            Level::INFO,
            "Info event",
            Some("module_a"),
            Some("test.rs"),
            Some(42),
            None,
        );

        let error_event = create_test_event(
            2,
            Level::ERROR,
            "Error event",
            Some("module_a"),
            Some("test.rs"),
            Some(43),
            None,
        );

        // Send events to the tracer
        send_event(&tracer, info_event).await;
        send_event(&tracer, error_event).await;

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check callback invocations
        {
            let data = callback_data.lock().await;

            // Should have 2 callback invocations
            assert_eq!(data.len(), 2, "Should have exactly 2 callback invocations");

            // Find the info event callback
            let info_callback = data.iter().find(|d| d.event == "Info event");
            assert!(
                info_callback.is_some(),
                "Should have called back for info event"
            );

            // Info event should only be captured by lax_tab
            if let Some(info_cb) = info_callback {
                assert_eq!(
                    info_cb.tabs.len(),
                    1,
                    "Info event should be captured by exactly 1 tab"
                );
                assert!(
                    info_cb.tabs.contains(&"lax_tab".to_string()),
                    "Info event should be captured by lax_tab"
                );
            }

            // Find the error event callback
            let error_callback = data.iter().find(|d| d.event == "Error event");
            assert!(
                error_callback.is_some(),
                "Should have called back for error event"
            );

            // Error event should be captured by both tabs
            if let Some(error_cb) = error_callback {
                assert_eq!(
                    error_cb.tabs.len(),
                    2,
                    "Error event should be captured by exactly 2 tabs"
                );
                assert!(
                    error_cb.tabs.contains(&"lax_tab".to_string()),
                    "Error event should be captured by lax_tab"
                );
                assert!(
                    error_cb.tabs.contains(&"strict_tab".to_string()),
                    "Error event should be captured by strict_tab"
                );
            }
        }

        // Update the callback and ensure it applies correctly
        let new_callback_data = Arc::new(Mutex::new(Vec::<CallbackData>::new()));
        let new_data_clone = new_callback_data.clone();

        tracer
            .set_callback(move |event, tabs| {
                let data = new_data_clone.clone();
                let msg = event.message.clone();
                let sub_list: Vec<String> = tabs.iter().map(|&s| s.to_string()).collect();

                tokio::spawn(async move {
                    let mut lock = data.lock().await;
                    lock.push(CallbackData {
                        event: msg,
                        tabs: sub_list,
                    });
                });
            })?
            .await??;

        // Create a new event to test new callback
        let new_event = create_test_event(
            3,
            Level::INFO,
            "New event",
            Some("module_a"),
            Some("test.rs"),
            Some(44),
            None,
        );

        // Send the new event
        send_event(&tracer, new_event).await;

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check new callback invocations
        {
            let data = new_callback_data.lock().await;

            // Check for new event
            let new_msg_callback = data.iter().find(|d| d.event == "New event");
            assert!(
                new_msg_callback.is_some(),
                "Should have called back for new event"
            );

            if let Some(new_cb) = new_msg_callback {
                assert_eq!(
                    new_cb.tabs.len(),
                    1,
                    "New event should be captured by exactly 1 tab"
                );
                assert!(
                    new_cb.tabs.contains(&"lax_tab".to_string()),
                    "New event should be captured by lax_tab"
                );
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_specialized_callbacks() -> Result<()> {
        // Create a normal filter for the first tab using builder pattern
        let mut normal_matcher = MatcherSet::empty();
        normal_matcher.add_matcher(Matcher::info().module_pattern("test_*"));

        // Create a silencing filter for the second tab
        let mut silencing_matcher = MatcherSet::empty();
        // First add the explicit exclusion filter
        silencing_matcher.add_matcher(Matcher::info().exclude().module_pattern("test_silenced*"));
        // Then add the inclusion filter
        silencing_matcher.add_matcher(Matcher::info().module_pattern("test_*"));

        // Create a config with the tabs
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("normal_sub".to_string()).with_matcher_set(normal_matcher),
                TracerTab::new("silencing_sub".to_string()).with_matcher_set(silencing_matcher),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Track events with separate counters
        let captured_events = Arc::new(Mutex::new(Vec::<(String, Vec<String>)>::new()));
        let silenced_events = Arc::new(Mutex::new(Vec::<(String, Vec<String>)>::new()));
        let dropped_events = Arc::new(Mutex::new(Vec::<String>::new()));

        let captured_clone = captured_events.clone();
        let silenced_clone = silenced_events.clone();
        let dropped_clone = dropped_events.clone();

        // Set up the callbacks
        tracer
            .set_callback(move |event, tabs| {
                let events = captured_clone.clone();
                let msg = event.message.clone();
                let subs: Vec<String> = tabs.iter().map(|&s| s.to_string()).collect();

                tokio::spawn(async move {
                    let mut lock = events.lock().await;
                    lock.push((msg, subs));
                });
            })?
            .await??;

        tracer
            .set_silenced_callback(move |event, silencers| {
                println!("SILENCED CB: {} for {:?}", event.message, silencers);

                let events = silenced_clone.clone();
                let msg = event.message.clone();
                let subs: Vec<String> = silencers.iter().map(|&s| s.to_string()).collect();

                tokio::spawn(async move {
                    let mut lock = events.lock().await;
                    lock.push((msg, subs));
                });
            })?
            .await??;

        tracer
            .set_dropped_callback(move |event| {
                let events = dropped_clone.clone();
                let msg = event.message.clone();

                tokio::spawn(async move {
                    let mut lock = events.lock().await;
                    lock.push(msg);
                });
            })?
            .await??;

        // Create test events
        let normal_event = create_test_event(
            1,
            Level::INFO,
            "Normal event",
            Some("test_normal"),
            Some("test.rs"),
            Some(10),
            None,
        );

        // This event is specifically for testing silencing
        // We'll send it separately after removing normal_sub
        let silenced_event = create_test_event(
            2,
            Level::INFO,
            "Silenced event",
            Some("test_silenced"),
            Some("test.rs"),
            Some(20),
            None,
        );

        let dropped_event = create_test_event(
            3,
            Level::INFO,
            "Dropped event",
            Some("other_module"),
            Some("test.rs"),
            Some(30),
            None,
        );

        // Send normal and dropped events
        println!("Sending normal event...");
        tracer
            ._get_sender_for_testing()
            .send(normal_event)
            .expect("Failed to send");
        tokio::time::sleep(Duration::from_millis(100)).await;

        println!("Sending dropped event...");
        tracer
            ._get_sender_for_testing()
            .send(dropped_event)
            .expect("Failed to send");
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Remove normal_sub so the silenced event won't be captured by anyone
        tracer.remove_tab("normal_sub")?.await??;

        // Now send the silenced event - it should only be processed by silencing_sub
        // which will silence it, and since no other tab captures it,
        // its overall status should be SILENCED
        println!("Sending silenced event after removing normal_sub...");
        tracer
            ._get_sender_for_testing()
            .send(silenced_event)
            .expect("Failed to send");

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Check results
        let captured = captured_events.lock().await;
        let silenced = silenced_events.lock().await;
        let dropped = dropped_events.lock().await;

        println!("CAPTURED: {:?}", *captured);
        println!("SILENCED: {:?}", *silenced);
        println!("DROPPED: {:?}", *dropped);
        println!(
            "STATS - Captured: {}, Silenced: {}, Dropped: {}",
            tracer.get_captured_count(),
            tracer.get_silenced_count(),
            tracer.get_dropped_count()
        );

        // Verify captured events
        assert_eq!(captured.len(), 1, "Should have 1 captured event");

        // The normal event should be captured by both tabs
        if !captured.is_empty() {
            let (msg, tabs) = &captured[0];
            assert_eq!(msg, "Normal event", "Should be the normal event");
            assert_eq!(tabs.len(), 2, "Both tabs should capture it");
            assert!(
                tabs.contains(&"normal_sub".to_string()),
                "normal_sub should capture it"
            );
            assert!(
                tabs.contains(&"silencing_sub".to_string()),
                "silencing_sub should capture it"
            );
        }

        // Verify silenced events
        assert_eq!(silenced.len(), 1, "Should have 1 silenced event");

        if !silenced.is_empty() {
            let (msg, silencers) = &silenced[0];
            assert_eq!(msg, "Silenced event", "Should be the silenced event");
            assert_eq!(silencers.len(), 1, "Should be silenced by one tab");
            assert!(
                silencers.contains(&"silencing_sub".to_string()),
                "Should be silenced by silencing_sub"
            );
        }

        // Verify dropped events
        assert_eq!(dropped.len(), 1, "Should have 1 dropped event");
        assert!(
            dropped.contains(&"Dropped event".to_string()),
            "Should contain the dropped event"
        );

        // Also verify the counters match
        assert_eq!(
            tracer.get_captured_count(),
            1,
            "Tracer should report 1 captured event"
        );
        assert_eq!(
            tracer.get_silenced_count(),
            1,
            "Tracer should report 1 silenced event"
        );
        assert_eq!(
            tracer.get_dropped_count(),
            1,
            "Tracer should report 1 dropped event"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_silenced_events() -> Result<()> {
        // Create a tab with explicit silencing using builder pattern
        let mut silencing_matcher = MatcherSet::empty();
        silencing_matcher.add_matcher(Matcher::info().module_pattern("test_*"));
        silencing_matcher.add_matcher(
            Matcher::info()
                .exclude() // This creates a silenced event
                .module_pattern("test_silenced*"),
        );

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("silencing_sub".to_string()).with_matcher_set(silencing_matcher),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Track silenced events
        let silenced_events =
            Arc::new(tokio::sync::Mutex::new(Vec::<(String, Vec<String>)>::new()));
        let silenced_clone = silenced_events.clone();

        // Set silenced callback - print detailed debug info
        tracer
            .set_silenced_callback(move |event, silencers| {
                println!(
                    "SILENCED CALLBACK INVOKED: event={}, silencers={:?}",
                    event.message, silencers
                );

                let events = silenced_clone.clone();
                let msg = event.message.clone();
                let silencer_names: Vec<String> =
                    silencers.iter().map(|&s| s.to_string()).collect();

                println!("SILENCED EVENT DETECTED: {msg}");

                tokio::spawn(async move {
                    let mut lock = events.lock().await;
                    lock.push((msg, silencer_names));
                    println!("SILENCED EVENT STORED");
                });
            })?
            .await??;

        // Create and send a silenced event
        let silenced_event = create_test_event(
            1,
            Level::INFO,
            "Test silenced event",
            Some("test_silenced"),
            Some("test.rs"),
            Some(20),
            None,
        );

        println!("SENDING SILENCED EVENT");
        tracer
            ._get_sender_for_testing()
            .send(silenced_event.clone())
            .expect("Failed to send");

        // Allow time for processing
        println!("WAITING FOR PROCESSING");
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Check results
        let silenced = silenced_events.lock().await;
        println!("SILENCED EVENTS: {:?}", *silenced);

        // Check tracer statistics
        let silenced_count = tracer.get_silenced_count();
        println!("TRACER SILENCED COUNT: {silenced_count}");

        // This assertion should succeed if the silenced callback is working
        assert!(
            silenced_count > 0,
            "Tracer should report at least one silenced event"
        );

        assert_eq!(
            silenced.len(),
            1,
            "Should have exactly 1 silenced event in the callback results"
        );

        if !silenced.is_empty() {
            let (msg, silencers) = &silenced[0];
            assert_eq!(
                msg, "Test silenced event",
                "Should be the test silenced event"
            );
            assert_eq!(silencers.len(), 1, "Should be silenced by one tab");
            assert!(
                silencers.contains(&"silencing_sub".to_string()),
                "Should be silenced by silencing_sub"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_basic_message_capture() -> Result<()> {
        // Setup a basic capture filter using builder pattern
        let mut matcher_set = MatcherSet::empty();
        matcher_set.add_matcher(Matcher::debug().module_pattern("test_module*"));

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![TracerTab::new("test_tab".to_string()).with_matcher_set(matcher_set)],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Create captured messages tracker
        let captured_messages = Arc::new(Mutex::new(Vec::<TraceEvent>::new()));
        let captured_clone = captured_messages.clone();

        // Set callback to track captured messages
        tracer
            .set_callback(move |event, tabs| {
                if tabs.contains(&"test_tab") {
                    let captured = captured_clone.clone();
                    tokio::spawn(async move {
                        let mut lock = captured.lock().await;
                        lock.push(Arc::clone(&event));
                    });
                }
            })?
            .await??;

        // Create and submit matching test events
        let event1 = create_test_event(
            1,
            Level::INFO,
            "Test event 1",
            Some("test_module"),
            Some("test.rs"),
            Some(42),
            None,
        );
        let event2 = create_test_event(
            2,
            Level::WARN,
            "Test event 2",
            Some("test_module_other"),
            Some("test.rs"),
            Some(43),
            Some("test_span"),
        );

        // Non-matching event (wrong module)
        let event3 = create_test_event(
            3,
            Level::ERROR,
            "Test event 3",
            Some("other_module"),
            Some("test.rs"),
            Some(44),
            None,
        );

        // Send events to the tracer
        send_event(&tracer, event1).await;
        send_event(&tracer, event2).await;
        send_event(&tracer, event3).await;
        // Allow some time for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check captured messages
        let locked_captured = captured_messages.lock().await;
        assert_eq!(
            locked_captured.len(),
            2,
            "Should have captured exactly 2 messages"
        );

        // Verify dropped count is 1 (event3)
        let dropped_count = tracer.get_dropped_count();
        assert_eq!(dropped_count, 1, "Should have 1 dropped event");

        Ok(())
    }

    #[tokio::test]
    async fn test_matcher_behavior() -> Result<()> {
        // Create filter with both include and exclude filters using builder pattern
        let mut matcher_set = MatcherSet::empty();

        // Include all messages from test_module
        matcher_set.add_matcher(Matcher::trace().module_pattern("test_module*"));

        // But exclude any test_module_internal
        matcher_set.add_matcher(
            Matcher::trace()
                .exclude()
                .module_pattern("test_module_internal*"),
        );

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![TracerTab::new("test_tab".to_string()).with_matcher_set(matcher_set)],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Track messages - use more precise tracking
        let captured_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let silenced_messages = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_clone = captured_messages.clone();
        let silenced_clone = silenced_messages.clone();

        // Set callback
        tracer
            .set_callback(move |event, tabs| {
                if tabs.contains(&"test_tab") {
                    let captured = captured_clone.clone();
                    let msg = event.message.clone();
                    tokio::spawn(async move {
                        let mut lock = captured.lock().await;
                        lock.push(msg);
                    });
                }
            })?
            .await??;

        tracer
            .set_silenced_callback(move |event, _| {
                let captured = silenced_clone.clone();
                let msg = event.message.clone();
                tokio::spawn(async move {
                    let mut lock = captured.lock().await;
                    lock.push(msg);
                });
            })?
            .await??;

        // Create test events
        let should_capture = create_test_event(
            1,
            Level::INFO,
            "Should be captured",
            Some("test_module"),
            Some("test.rs"),
            Some(42),
            None,
        );

        let should_silence = create_test_event(
            2,
            Level::INFO,
            "Should be silenced",
            Some("test_module_internal"),
            Some("test.rs"),
            Some(43),
            None,
        );

        let another_capture = create_test_event(
            3,
            Level::DEBUG,
            "Should be captured too",
            Some("test_module_other"),
            Some("test.rs"),
            Some(44),
            None,
        );

        // Directly send events to the tracer
        let event_tx = tracer._get_sender_for_testing();
        let _ = event_tx.send(should_capture.clone());
        let _ = event_tx.send(should_silence.clone());
        let _ = event_tx.send(another_capture.clone());

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check captured messages
        {
            let captured = captured_messages.lock().await;
            let silenced = silenced_messages.lock().await;

            // Debug print
            println!("Captured messages: {:?}", *captured);
            println!("Silenced messages: {:?}", *silenced);

            assert_eq!(captured.len(), 2, "Should have captured 2 messages");
            assert_eq!(silenced.len(), 1, "Should have silenced 1 event");

            assert!(
                captured.contains(&"Should be captured".to_string()),
                "First event should be captured"
            );
            assert!(
                captured.contains(&"Should be captured too".to_string()),
                "Third event should be captured"
            );
            assert!(
                silenced.contains(&"Should be silenced".to_string()),
                "Second event should be silenced"
            );
        }

        // Check silenced count from tracer stats
        let tracer_silenced_count = tracer.get_silenced_count();
        assert_eq!(
            tracer_silenced_count, 1,
            "Tracer should report 1 silenced event"
        );

        // Test clear stats
        tracer.clear_stats()?.await??;

        // Check counts after clearing
        let silenced_after_clear = tracer.get_silenced_count();
        let dropped_after_clear = tracer.get_dropped_count();

        assert_eq!(
            silenced_after_clear, 0,
            "Silenced count should be 0 after clear"
        );
        assert_eq!(
            dropped_after_clear, 0,
            "Dropped count should be 0 after clear"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_matcher_level_behavior() -> Result<()> {
        // Test specifically how different log levels interact with filters
        // Create a filter set with specific level using builder pattern
        let mut matcher_set = MatcherSet::empty();
        matcher_set.add_matcher(
            Matcher::info() // Only INFO and higher
                .module_pattern("test_module*"),
        );

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![TracerTab::new("level_test_tab".to_string()).with_matcher_set(matcher_set)],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Track captured events by level for first phase
        let first_phase_events = Arc::new(Mutex::new(Vec::<(String, TraceLevel)>::new()));
        let first_clone = first_phase_events.clone();

        // Set callback for first phase
        tracer
            .set_callback(move |event, _tabs| {
                let captured = first_clone.clone();
                let msg = event.message.clone();
                let level = event.level;

                tokio::spawn(async move {
                    let mut lock = captured.lock().await;
                    lock.push((msg, level));
                });
            })?
            .await??;

        // Create events at different levels - first phase
        let levels = [
            (Level::TRACE, "TRACE level event 1"),
            (Level::DEBUG, "DEBUG level event 1"),
            (Level::INFO, "INFO level event 1"),
            (Level::WARN, "WARN level event 1"),
            (Level::ERROR, "ERROR level event 1"),
        ];

        for (i, (level, msg)) in levels.iter().enumerate() {
            let event = create_test_event(
                i as u64,
                *level,
                msg,
                Some("test_module"),
                Some("test.rs"),
                Some(i as u32),
                None,
            );

            let _ = tracer._get_sender_for_testing().send(event);
        }

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify only INFO, WARN, and ERROR were captured
        {
            let events = first_phase_events.lock().await;

            // Should have exactly 3 events (INFO, WARN, ERROR)
            assert_eq!(events.len(), 3, "Should have captured exactly 3 events");

            // Check specific levels
            let has_info = events.iter().any(|(msg, _)| msg == "INFO level event 1");
            let has_warn = events.iter().any(|(msg, _)| msg == "WARN level event 1");
            let has_error = events.iter().any(|(msg, _)| msg == "ERROR level event 1");
            let has_debug = events.iter().any(|(msg, _)| msg == "DEBUG level event 1");
            let has_trace = events.iter().any(|(msg, _)| msg == "TRACE level event 1");

            assert!(has_info, "Should have captured INFO event");
            assert!(has_warn, "Should have captured WARN event");
            assert!(has_error, "Should have captured ERROR event");
            assert!(!has_debug, "Should NOT have captured DEBUG event");
            assert!(!has_trace, "Should NOT have captured TRACE event");
        }

        // Clear the tracer's stats to start fresh
        tracer.clear_stats()?.await??;

        // Now update the tab to include DEBUG level using builder pattern
        let mut updated_matcher = MatcherSet::empty();
        updated_matcher.add_matcher(
            Matcher::debug() // DEBUG and higher
                .module_pattern("test_module*"),
        );

        // Track captured events for second phase separately
        let second_phase_events = Arc::new(Mutex::new(Vec::<(String, TraceLevel)>::new()));
        let second_clone = second_phase_events.clone();

        // Update callback for second phase
        tracer
            .set_callback(move |event, _tabs| {
                let captured = second_clone.clone();
                let msg = event.message.clone();
                let level = event.level;

                tokio::spawn(async move {
                    let mut lock = captured.lock().await;
                    lock.push((msg, level));
                });
            })?
            .await??;

        // Update the tab filter
        tracer
            .update_tab("level_test_tab", updated_matcher)?
            .await??;

        // Send new events with different IDs and messages for the second phase
        let levels2 = [
            (Level::TRACE, "TRACE level event 2"),
            (Level::DEBUG, "DEBUG level event 2"),
            (Level::INFO, "INFO level event 2"),
            (Level::WARN, "WARN level event 2"),
            (Level::ERROR, "ERROR level event 2"),
        ];

        for (i, (level, msg)) in levels2.iter().enumerate() {
            let event = create_test_event(
                (i + 100) as u64, // Different IDs
                *level,
                msg,
                Some("test_module"),
                Some("test.rs"),
                Some((i + 100) as u32),
                None,
            );

            let _ = tracer._get_sender_for_testing().send(event);
        }

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify now DEBUG, INFO, WARN, and ERROR were captured
        {
            let events = second_phase_events.lock().await;

            // Print events for debugging
            println!("Second phase events: {events:?}");

            // Should have exactly 4 events (DEBUG, INFO, WARN, ERROR)
            assert_eq!(events.len(), 4, "Should have captured exactly 4 events");

            // Check specific levels
            let has_info = events.iter().any(|(msg, _)| msg == "INFO level event 2");
            let has_warn = events.iter().any(|(msg, _)| msg == "WARN level event 2");
            let has_error = events.iter().any(|(msg, _)| msg == "ERROR level event 2");
            let has_debug = events.iter().any(|(msg, _)| msg == "DEBUG level event 2");
            let has_trace = events.iter().any(|(msg, _)| msg == "TRACE level event 2");

            assert!(has_info, "Should have captured INFO event");
            assert!(has_warn, "Should have captured WARN event");
            assert!(has_error, "Should have captured ERROR event");
            assert!(has_debug, "Should have captured DEBUG event");
            assert!(!has_trace, "Should NOT have captured TRACE event");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_tracer_under_heavy_load() -> Result<()> {
        // Test tracer performance with a high volume of events
        // Create a basic filter set using builder pattern
        let mut matcher_set = MatcherSet::empty();
        matcher_set.add_matcher(Matcher::debug().module_pattern("test_module*"));

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![TracerTab::new("test_tab".to_string()).with_matcher_set(matcher_set)],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Create a counter for captured events
        let captured_count = Arc::new(Mutex::new(0));
        let count_clone = captured_count.clone();

        // Create a oneshot channel to signal when the final event is processed
        let (completion_tx, completion_rx) = tokio::sync::oneshot::channel::<()>();
        let completion_tx = Arc::new(Mutex::new(Some(completion_tx)));
        let completion_tx_clone = completion_tx.clone();

        // Set callback
        tracer
            .set_callback(move |event, _tabs| {
                let counter = count_clone.clone();
                let completion_sender = completion_tx_clone.clone();

                tokio::spawn(async move {
                    let mut lock = counter.lock().await;
                    *lock += 1;

                    // Check if this is the sentinel event (special ID for the final event)
                    if event.id == u64::MAX {
                        // Signal completion
                        let mut sender = completion_sender.lock().await;
                        if let Some(tx) = sender.take() {
                            let _ = tx.send(());
                        }
                    }
                });
            })?
            .await??;

        // Send a large number of events (1000)
        const EVENT_COUNT: u64 = 1000;
        let tx = tracer._get_sender_for_testing();

        for i in 0..EVENT_COUNT {
            let event = create_test_event(
                i,
                Level::DEBUG,
                &format!("Test event {i}"),
                Some("test_module"),
                Some("test.rs"),
                Some(i as u32),
                None,
            );
            let _ = tx.send(event);
        }

        // Send the sentinel event with a special ID
        let sentinel_event = create_test_event(
            u64::MAX, // Special ID that won't conflict with regular events
            Level::DEBUG,
            "SENTINEL EVENT - END OF TEST",
            Some("test_module"),
            Some("test.rs"),
            Some(0),
            None,
        );
        let _ = tx.send(sentinel_event);

        // Wait for the completion signal with a timeout
        match tokio::time::timeout(Duration::from_secs(15), completion_rx).await {
            Ok(_) => {
                println!("Received completion signal, all events processed");
            }
            Err(_) => {
                println!("Timed out waiting for completion signal");
            }
        }

        // Verify that all events were captured
        {
            let count = captured_count.lock().await;
            // Subtract 1 to exclude the sentinel event from our count
            let actual_count = *count - 1;

            println!("Processed {actual_count} regular events (plus 1 sentinel)");

            assert_eq!(
                actual_count, EVENT_COUNT as usize,
                "Should have captured all {EVENT_COUNT} events"
            );
        }

        // Also verify captured count from tracer stats
        let tracer_captured_count = tracer.get_captured_count();
        // Subtract 1 for the sentinel event
        assert_eq!(
            tracer_captured_count - 1,
            EVENT_COUNT,
            "Tracer should report having captured all events"
        );

        Ok(())
    }

    // Test span-based filtering specifically
    #[tokio::test]
    async fn test_span_based_matchering() -> Result<()> {
        // Create filter set that includes specific spans using builder pattern
        let mut span_matcher = MatcherSet::empty();
        span_matcher.add_matcher(
            Matcher::info().all_modules().span_pattern("important*"), // Only include spans that start with "important"
        );

        // Create filter set that excludes specific spans
        let mut exclude_span_matcher = MatcherSet::empty();
        // First include everything
        exclude_span_matcher.add_matcher(Matcher::info().all_modules());
        // Then exclude specific spans
        exclude_span_matcher.add_matcher(
            Matcher::info().exclude().span_pattern("ignore*"), // Exclude spans that start with "ignore"
        );

        // Create config with the tabs
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("span_tab".to_string()).with_matcher_set(span_matcher),
                TracerTab::new("exclude_tab".to_string()).with_matcher_set(exclude_span_matcher),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Track captured events by tab
        let captured_by_span = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let captured_by_exclude = Arc::new(Mutex::new(Vec::<(String, String)>::new()));

        let span_clone = captured_by_span.clone();
        let exclude_clone = captured_by_exclude.clone();

        // Set callback
        tracer
            .set_callback(move |event, tabs| {
                let span_name = event.span_name.clone().unwrap_or_default();
                let msg = event.message.clone();

                for &tab in tabs {
                    if tab.eq("span_tab") {
                        let span_captured = span_clone.clone();
                        let msg_clone = msg.clone();
                        let span_name_clone = span_name.clone();
                        tokio::spawn(async move {
                            let mut lock = span_captured.lock().await;
                            lock.push((msg_clone.clone(), span_name_clone.clone()));
                        });
                    } else if tab.eq("exclude_tab") {
                        let exclude_captured = exclude_clone.clone();
                        let msg_clone = msg.clone();
                        let span_name_clone = span_name.clone();
                        tokio::spawn(async move {
                            let mut lock = exclude_captured.lock().await;
                            lock.push((msg_clone.clone(), span_name_clone.clone()));
                        });
                    }
                }
            })?
            .await??;

        // Create test events with different span names
        let important_event = create_test_event(
            1,
            Level::INFO,
            "Important event",
            Some("test_module"),
            Some("test.rs"),
            Some(42),
            Some("important_span"),
        );

        let normal_event = create_test_event(
            2,
            Level::INFO,
            "Normal event",
            Some("test_module"),
            Some("test.rs"),
            Some(43),
            Some("normal_span"),
        );

        let ignore_event = create_test_event(
            3,
            Level::INFO,
            "Ignore event",
            Some("test_module"),
            Some("test.rs"),
            Some(44),
            Some("ignore_span"),
        );

        let no_span_event = create_test_event(
            4,
            Level::INFO,
            "No span event",
            Some("test_module"),
            Some("test.rs"),
            Some(45),
            None,
        );

        // Send all events
        let tx = tracer._get_sender_for_testing();
        let _ = tx.send(important_event);
        let _ = tx.send(normal_event);
        let _ = tx.send(ignore_event);
        let _ = tx.send(no_span_event);

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify correct events were captured
        {
            let span_events = captured_by_span.lock().await;
            let exclude_events = captured_by_exclude.lock().await;

            println!("Span tab events: {:?}", *span_events);
            println!("Exclude tab events: {:?}", *exclude_events);

            // span_tab should only capture events with "important" span
            assert_eq!(span_events.len(), 1, "Span tab should only capture 1 event");
            assert!(
                span_events
                    .iter()
                    .any(|(msg, span)| msg == "Important event" && span == "important_span"),
                "Span tab should capture the important event"
            );

            // exclude_tab should capture all events except those with "ignore" span
            assert_eq!(
                exclude_events.len(),
                3,
                "Exclude tab should capture 3 events"
            );

            // Should have the important and normal span events plus the no-span event
            assert!(
                exclude_events
                    .iter()
                    .any(|(msg, _)| msg == "Important event"),
                "Exclude tab should include important event"
            );
            assert!(
                exclude_events.iter().any(|(msg, _)| msg == "Normal event"),
                "Exclude tab should include normal event"
            );
            assert!(
                exclude_events.iter().any(|(msg, _)| msg == "No span event"),
                "Exclude tab should include no-span event"
            );
            assert!(
                !exclude_events.iter().any(|(msg, _)| msg == "Ignore event"),
                "Exclude tab should NOT include ignore event"
            );
        }

        Ok(())
    }

    // Test target pattern filtering specifically
    #[tokio::test]
    async fn test_target_pattern_matchering() -> Result<()> {
        // Create filter set that includes specific targets
        let mut target_matcher = MatcherSet::empty();
        target_matcher.add_matcher(
            Matcher::info().all_modules().target_pattern("api*"), // Only include targets that start with "api"
        );

        // Create filter set that excludes specific targets
        let mut exclude_target_matcher = MatcherSet::empty();
        // First include everything
        exclude_target_matcher.add_matcher(Matcher::info().all_modules());
        // Then exclude specific targets
        exclude_target_matcher.add_matcher(
            Matcher::info().exclude().target_pattern("internal*"), // Exclude targets that start with "internal"
        );

        // Create config with the tabs
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("target_tab".to_string()).with_matcher_set(target_matcher),
                TracerTab::new("exclude_target_tab".to_string())
                    .with_matcher_set(exclude_target_matcher),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Create test events with different targets
        let api_event = create_test_event_with_target(
            1,
            Level::INFO,
            "API event",
            Some("test_module"),
            "api_service",
            Some("test.rs"),
            Some(10),
            None,
        );

        let web_event = create_test_event_with_target(
            2,
            Level::INFO,
            "Web event",
            Some("test_module"),
            "web_service",
            Some("test.rs"),
            Some(11),
            None,
        );

        let internal_event = create_test_event_with_target(
            3,
            Level::INFO,
            "Internal event",
            Some("test_module"),
            "internal_service",
            Some("test.rs"),
            Some(12),
            None,
        );

        // Track captured events by tab
        let target_captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let exclude_captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let target_clone = target_captured.clone();
        let exclude_clone = exclude_captured.clone();

        // Set callback
        tracer
            .set_callback(move |event, tabs| {
                let msg = event.message.clone();

                for &tab in tabs {
                    if tab.eq("target_tab") {
                        let captured = target_clone.clone();
                        let msg_clone = msg.clone();
                        tokio::spawn(async move {
                            let mut lock = captured.lock().await;
                            lock.push(msg_clone);
                        });
                    } else if tab.eq("exclude_target_tab") {
                        let captured = exclude_clone.clone();
                        let msg_clone = msg.clone();
                        tokio::spawn(async move {
                            let mut lock = captured.lock().await;
                            lock.push(msg_clone);
                        });
                    }
                }
            })?
            .await??;

        // Send events
        let tx = tracer._get_sender_for_testing();
        let _ = tx.send(api_event);
        let _ = tx.send(web_event);
        let _ = tx.send(internal_event);

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify correct events were captured
        {
            let target_events = target_captured.lock().await;
            let exclude_events = exclude_captured.lock().await;

            println!("Target tab events: {:?}", *target_events);
            println!("Exclude target tab events: {:?}", *exclude_events);

            // target_tab should only capture events with "api" target
            assert_eq!(
                target_events.len(),
                1,
                "Target tab should only capture 1 event"
            );
            assert!(
                target_events.contains(&"API event".to_string()),
                "Target tab should capture the API event"
            );

            // exclude_target_tab should capture all events except those with "internal" target
            assert_eq!(
                exclude_events.len(),
                2,
                "Exclude target tab should capture 2 events"
            );
            assert!(
                exclude_events.contains(&"API event".to_string()),
                "Exclude target tab should include api event"
            );
            assert!(
                exclude_events.contains(&"Web event".to_string()),
                "Exclude target tab should include web event"
            );
            assert!(
                !exclude_events.contains(&"Internal event".to_string()),
                "Exclude target tab should NOT include internal event"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_combined_target_and_module_matchering() -> Result<()> {
        // Create combined filters that match different combinations
        let mut combined_matcher = MatcherSet::empty();
        combined_matcher.add_matcher(
            Matcher::info()
                .module_pattern("service_*")
                .target_pattern("api_*"),
        );

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("combined_tab".to_string()).with_matcher_set(combined_matcher),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Create events with different combinations of module and target
        let match_both = create_test_event_with_target(
            1,
            Level::INFO,
            "Match both module and target",
            Some("service_module"),
            "api_service",
            Some("test.rs"),
            Some(10),
            None,
        );

        let match_module_only = create_test_event_with_target(
            2,
            Level::INFO,
            "Match module only",
            Some("service_module"),
            "web_service",
            Some("test.rs"),
            Some(11),
            None,
        );

        let match_target_only = create_test_event_with_target(
            3,
            Level::INFO,
            "Match target only",
            Some("utility_module"),
            "api_service",
            Some("test.rs"),
            Some(12),
            None,
        );

        let match_neither = create_test_event_with_target(
            4,
            Level::INFO,
            "Match neither",
            Some("utility_module"),
            "web_service",
            Some("test.rs"),
            Some(13),
            None,
        );

        // Track captured events
        let captured_events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_clone = captured_events.clone();

        // Set callback
        tracer
            .set_callback(move |event, _| {
                let captured = captured_clone.clone();
                let msg = event.message.clone();

                tokio::spawn(async move {
                    let mut lock = captured.lock().await;
                    lock.push(msg);
                });
            })?
            .await??;

        // Send events
        let tx = tracer._get_sender_for_testing();
        let _ = tx.send(match_both);
        let _ = tx.send(match_module_only);
        let _ = tx.send(match_target_only);
        let _ = tx.send(match_neither);

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify only the event matching both conditions was captured
        {
            let events = captured_events.lock().await;

            println!("Combined filter events: {:?}", *events);

            assert_eq!(
                events.len(),
                1,
                "Combined filter should only capture 1 event"
            );
            assert!(
                events.contains(&"Match both module and target".to_string()),
                "Should only capture the event matching both module and target"
            );
            assert!(
                !events.contains(&"Match module only".to_string()),
                "Should not capture event matching only module"
            );
            assert!(
                !events.contains(&"Match target only".to_string()),
                "Should not capture event matching only target"
            );
            assert!(
                !events.contains(&"Match neither".to_string()),
                "Should not capture event matching neither"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_target_patterns() -> Result<()> {
        // Create a filter with multiple target patterns
        let mut multi_target_matcher = MatcherSet::empty();
        multi_target_matcher.add_matcher(
            Matcher::info()
                .all_modules()
                .target_patterns(vec!["api_*", "web_*"]), // Match both api and web targets
        );

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("multi_target_tab".to_string())
                    .with_matcher_set(multi_target_matcher),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Create events with different targets
        let api_event = create_test_event_with_target(
            1,
            Level::INFO,
            "API event",
            Some("test_module"),
            "api_service",
            Some("test.rs"),
            Some(10),
            None,
        );

        let web_event = create_test_event_with_target(
            2,
            Level::INFO,
            "Web event",
            Some("test_module"),
            "web_service",
            Some("test.rs"),
            Some(11),
            None,
        );

        let db_event = create_test_event_with_target(
            3,
            Level::INFO,
            "DB event",
            Some("test_module"),
            "db_service",
            Some("test.rs"),
            Some(12),
            None,
        );

        // Track captured events
        let captured_events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_clone = captured_events.clone();

        // Set callback
        tracer
            .set_callback(move |event, _| {
                let captured = captured_clone.clone();
                let msg = event.message.clone();

                tokio::spawn(async move {
                    let mut lock = captured.lock().await;
                    lock.push(msg);
                });
            })?
            .await??;

        // Send events
        let tx = tracer._get_sender_for_testing();
        let _ = tx.send(api_event);
        let _ = tx.send(web_event);
        let _ = tx.send(db_event);

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify only the api and web events were captured
        {
            let events = captured_events.lock().await;

            println!("Multi-target filter events: {:?}", *events);

            assert_eq!(
                events.len(),
                2,
                "Multi-target filter should capture exactly 2 events"
            );
            assert!(
                events.contains(&"API event".to_string()),
                "Should capture the API event"
            );
            assert!(
                events.contains(&"Web event".to_string()),
                "Should capture the Web event"
            );
            assert!(
                !events.contains(&"DB event".to_string()),
                "Should not capture the DB event"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_target_pattern_wildcard() -> Result<()> {
        // Create a filter with wildcard target pattern
        let mut wildcard_matcher = MatcherSet::empty();
        wildcard_matcher.add_matcher(
            Matcher::info().all_modules(), // Match all targets
        );

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("wildcard_tab".to_string()).with_matcher_set(wildcard_matcher),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Create events with different targets
        let events = [
            create_test_event_with_target(
                1,
                Level::INFO,
                "API event",
                Some("test_module"),
                "api_service",
                Some("test.rs"),
                Some(10),
                None,
            ),
            create_test_event_with_target(
                2,
                Level::INFO,
                "Web event",
                Some("test_module"),
                "web_service",
                Some("test.rs"),
                Some(11),
                None,
            ),
            create_test_event_with_target(
                3,
                Level::INFO,
                "DB event",
                Some("test_module"),
                "db_service",
                Some("test.rs"),
                Some(12),
                None,
            ),
            create_test_event_with_target(
                4,
                Level::INFO,
                "Empty target",
                Some("test_module"),
                "",
                Some("test.rs"),
                Some(13),
                None,
            ),
        ];

        // Track captured events
        let captured_events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_clone = captured_events.clone();

        // Set callback
        tracer
            .set_callback(move |event, _| {
                let captured = captured_clone.clone();
                let msg = event.message.clone();

                tokio::spawn(async move {
                    let mut lock = captured.lock().await;
                    lock.push(msg);
                });
            })?
            .await??;

        // Send events
        let tx = tracer._get_sender_for_testing();
        for event in events.iter() {
            let _ = tx.send(event.clone());
        }

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify all events were captured
        {
            let events = captured_events.lock().await;

            println!("Wildcard target filter events: {:?}", *events);

            assert_eq!(
                events.len(),
                4,
                "Wildcard target filter should capture all events"
            );
            assert!(
                events.contains(&"API event".to_string()),
                "Should capture the API event"
            );
            assert!(
                events.contains(&"Web event".to_string()),
                "Should capture the Web event"
            );
            assert!(
                events.contains(&"DB event".to_string()),
                "Should capture the DB event"
            );
            assert!(
                events.contains(&"Empty target".to_string()),
                "Should capture event with empty target"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_complex_target_patterns() -> Result<()> {
        // Create a filter with complex target pattern
        let mut complex_matcher = MatcherSet::empty();
        complex_matcher.add_matcher(
            Matcher::info().all_modules().target_pattern("*_service_v1"), // Match targets ending with _service_v1
        );

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("complex_target_tab".to_string()).with_matcher_set(complex_matcher),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Create events with different targets
        let events = [
            create_test_event_with_target(
                1,
                Level::INFO,
                "API v1 event",
                Some("test_module"),
                "api_service_v1",
                Some("test.rs"),
                Some(10),
                None,
            ),
            create_test_event_with_target(
                2,
                Level::INFO,
                "API v2 event",
                Some("test_module"),
                "api_service_v2",
                Some("test.rs"),
                Some(11),
                None,
            ),
            create_test_event_with_target(
                3,
                Level::INFO,
                "Web v1 event",
                Some("test_module"),
                "web_service_v1",
                Some("test.rs"),
                Some(12),
                None,
            ),
            create_test_event_with_target(
                4,
                Level::INFO,
                "DB event",
                Some("test_module"),
                "db_service",
                Some("test.rs"),
                Some(13),
                None,
            ),
        ];

        // Track captured events
        let captured_events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_clone = captured_events.clone();

        // Set callback
        tracer
            .set_callback(move |event, _| {
                let captured = captured_clone.clone();
                let msg = event.message.clone();

                tokio::spawn(async move {
                    let mut lock = captured.lock().await;
                    lock.push(msg);
                });
            })?
            .await??;

        // Send events
        let tx = tracer._get_sender_for_testing();
        for event in events.iter() {
            let _ = tx.send(event.clone());
        }

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify only v1 service events were captured
        {
            let events = captured_events.lock().await;

            println!("Complex target filter events: {:?}", *events);

            assert_eq!(
                events.len(),
                2,
                "Complex target filter should capture exactly 2 events"
            );
            assert!(
                events.contains(&"API v1 event".to_string()),
                "Should capture the API v1 event"
            );
            assert!(
                events.contains(&"Web v1 event".to_string()),
                "Should capture the Web v1 event"
            );
            assert!(
                !events.contains(&"API v2 event".to_string()),
                "Should not capture the API v2 event"
            );
            assert!(
                !events.contains(&"DB event".to_string()),
                "Should not capture the DB event"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_combined_all_matcher_types() -> Result<()> {
        // Create a filter that combines all filter types
        let mut combined_matcher = MatcherSet::empty();
        combined_matcher.add_matcher(
            Matcher::info()
                .module_pattern("service_*")
                .file_pattern("*.rs")
                .span_pattern("operation_*")
                .target_pattern("api_*"),
        );

        // Create config with the tab
        let config = TracerConfig {
            tabs: vec![
                TracerTab::new("all_matchers_tab".to_string()).with_matcher_set(combined_matcher),
            ],
        };

        // Initialize tracer with the config
        let tracer = Tracer::new_with_config(config);

        // Create events with various combinations
        let perfect_match = create_test_event_with_target(
            1,
            Level::INFO,
            "Perfect match",
            Some("service_module"),
            "api_service",
            Some("test.rs"),
            Some(10),
            Some("operation_get"),
        );

        let miss_span = create_test_event_with_target(
            2,
            Level::INFO,
            "Missing span match",
            Some("service_module"),
            "api_service",
            Some("test.rs"),
            Some(11),
            Some("other_span"),
        );

        let miss_target = create_test_event_with_target(
            3,
            Level::INFO,
            "Missing target match",
            Some("service_module"),
            "web_service",
            Some("test.rs"),
            Some(12),
            Some("operation_post"),
        );

        let miss_module = create_test_event_with_target(
            4,
            Level::INFO,
            "Missing module match",
            Some("utility_module"),
            "api_service",
            Some("test.rs"),
            Some(13),
            Some("operation_delete"),
        );

        let miss_file = create_test_event_with_target(
            5,
            Level::INFO,
            "Missing file match",
            Some("service_module"),
            "api_service",
            Some("test.txt"),
            Some(14),
            Some("operation_put"),
        );

        // Track captured events
        let captured_events = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_clone = captured_events.clone();

        // Set callback
        tracer
            .set_callback(move |event, _| {
                let captured = captured_clone.clone();
                let msg = event.message.clone();

                tokio::spawn(async move {
                    let mut lock = captured.lock().await;
                    lock.push(msg);
                });
            })?
            .await??;

        // Send events
        let tx = tracer._get_sender_for_testing();
        let _ = tx.send(perfect_match);
        let _ = tx.send(miss_span);
        let _ = tx.send(miss_target);
        let _ = tx.send(miss_module);
        let _ = tx.send(miss_file);

        // Allow time for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify only the perfect match was captured
        {
            let events = captured_events.lock().await;

            println!("Combined all filter types events: {:?}", *events);

            assert_eq!(
                events.len(),
                1,
                "Combined filter should capture exactly 1 event"
            );
            assert!(
                events.contains(&"Perfect match".to_string()),
                "Should only capture the event matching all criteria"
            );
        }

        Ok(())
    }
}
