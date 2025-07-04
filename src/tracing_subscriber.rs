// src/tracing_subscriber.rs
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::mpsc;
use tracing::{
    Event, Metadata, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};
use tracing_core::subscriber::Interest;

use crate::{TraceData, TraceEvent};

// Complete span information storage - we need all of this for trace events
#[derive(Debug, Clone)]
struct SpanInfo {
    name: String,
    target: String,
    module_path: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    parent_id: Option<u64>,
    fields: HashMap<String, String>,
    metadata: &'static Metadata<'static>,
}

// Thread-local span stack for tracking current span context
thread_local! {
    static SPAN_STACK: std::cell::RefCell<Vec<u64>> = const { std::cell::RefCell::new(Vec::new()) };
}

// Custom subscriber that forwards events to our centralized dispatcher
pub struct TracingSubscriber {
    sender: mpsc::UnboundedSender<TraceEvent>,
    id_counter: Arc<AtomicU64>,
    span_storage: Arc<Mutex<HashMap<u64, SpanInfo>>>,
}

impl TracingSubscriber {
    pub fn new(sender: mpsc::UnboundedSender<TraceEvent>, id_counter: Arc<AtomicU64>) -> Self {
        Self {
            sender,
            id_counter,
            span_storage: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Get current span ID from thread-local stack
    fn current_span_id(&self) -> Option<u64> {
        SPAN_STACK.with(|stack| stack.borrow().last().copied())
    }

    // Build complete span hierarchy traversing parent relationships
    fn build_span_hierarchy(&self, span_id: u64) -> String {
        let storage = self.span_storage.lock().unwrap();
        let mut hierarchy = Vec::new();
        let mut current_id = Some(span_id);

        // Traverse up the parent chain
        while let Some(id) = current_id {
            if let Some(span_info) = storage.get(&id) {
                hierarchy.push(span_info.name.clone());
                current_id = span_info.parent_id;
            } else {
                break;
            }
        }

        // Reverse to get root-to-leaf order
        hierarchy.reverse();
        hierarchy.join("::")
    }

    // Get span info by ID - we need all the span info for the trace event
    fn get_span_info(&self, span_id: u64) -> Option<SpanInfo> {
        let storage = self.span_storage.lock().unwrap();
        storage.get(&span_id).cloned()
    }

    // Create complete field visitor for extracting all field data
    fn extract_fields(&self, attributes: &Attributes<'_>) -> HashMap<String, String> {
        let mut visitor = FieldVisitor::default();
        attributes.record(&mut visitor);
        visitor.fields
    }

    // Extract fields from a Record
    fn extract_record_fields(&self, record: &Record<'_>) -> HashMap<String, String> {
        let mut visitor = FieldVisitor::default();
        record.record(&mut visitor);
        visitor.fields
    }
}

// Complete field visitor implementation
#[derive(Default)]
struct FieldVisitor {
    fields: HashMap<String, String>,
    message: Option<String>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

impl Subscriber for TracingSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        // Always enable - let our dispatcher handle filtering
        true
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        let span_id_u64 = self.id_counter.fetch_add(1, Ordering::SeqCst);
        let span_id = Id::from_u64(span_id_u64);
        let metadata = span.metadata();

        // Get parent span ID from current context
        let parent_id = self.current_span_id();

        // Extract all fields from the span attributes
        let fields = self.extract_fields(span);

        // Create complete span info - we need all of this for trace events
        let span_info = SpanInfo {
            name: metadata.name().to_string(),
            target: metadata.target().to_string(),
            module_path: metadata.module_path().map(|s| s.to_string()),
            file: metadata.file().map(|s| s.to_string()),
            line: metadata.line(),
            parent_id,
            fields,
            metadata,
        };

        // Store span info for hierarchy tracking
        {
            let mut storage = self.span_storage.lock().unwrap();
            storage.insert(span_id_u64, span_info);
        }

        span_id
    }

    fn record(&self, span: &Id, values: &Record<'_>) {
        // Update span fields when new values are recorded
        let span_id_u64 = span.into_u64();
        let new_fields = self.extract_record_fields(values);

        if let Ok(mut storage) = self.span_storage.lock() {
            if let Some(span_info) = storage.get_mut(&span_id_u64) {
                // Merge new fields with existing ones
                span_info.fields.extend(new_fields);
            }
        }
    }

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {
        // Handle follows-from relationships
        // This is used for causal relationships between spans that aren't parent-child
        // For our filtering purposes, we don't need to handle this specially
    }

    fn event(&self, event: &Event<'_>) {
        // Generate a unique ID for this event
        let event_id = self.id_counter.fetch_add(1, Ordering::SeqCst);

        // Create the trace event with complete information
        let mut trace_data = TraceData::new(event_id, event);

        // Get current span context and enrich the trace event with span information
        if let Some(current_span_id) = self.current_span_id() {
            if let Some(span_info) = self.get_span_info(current_span_id) {
                // Set span-specific information
                trace_data.span_name = Some(span_info.name.clone());
                trace_data.span_hierarchy = Some(self.build_span_hierarchy(current_span_id));

                // If the event doesn't have its own module/file/line info, inherit from span
                if trace_data.module_path.is_none() && span_info.module_path.is_some() {
                    trace_data.module_path = span_info.module_path;
                }
                if trace_data.file.is_none() && span_info.file.is_some() {
                    trace_data.file = span_info.file;
                }
                if trace_data.line.is_none() && span_info.line.is_some() {
                    trace_data.line = span_info.line;
                }

                // Merge span fields with event fields (event fields take precedence)
                let mut combined_fields = span_info.fields;
                combined_fields.extend(trace_data.fields.clone());
                trace_data.fields = combined_fields;

                // Update target if the event target is generic but span has specific target
                if trace_data.target.is_empty() || trace_data.target == "unknown" {
                    trace_data.target = span_info.target;
                }
            }
        }

        // Wrap in Arc for sharing
        let trace_event = Arc::new(trace_data);

        // Send the event to the dispatcher (ignore send errors)
        let _ = self.sender.send(trace_event);
    }

    fn enter(&self, span: &Id) {
        // Push span onto thread-local stack when entered
        let span_id_u64 = span.into_u64();
        SPAN_STACK.with(|stack| {
            stack.borrow_mut().push(span_id_u64);
        });
    }

    fn exit(&self, span: &Id) {
        // Pop span from thread-local stack when exited
        let span_id_u64 = span.into_u64();
        SPAN_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            // Find and remove the span (should be the last one)
            if let Some(pos) = stack.iter().rposition(|&id| id == span_id_u64) {
                stack.remove(pos);
            }
        });
    }

    fn clone_span(&self, id: &Id) -> Id {
        // Return the same ID - spans are reference counted internally by tracing
        id.clone()
    }

    fn drop_span(&self, id: Id) {
        // Clean up span data when it's dropped
        let span_id_u64 = id.into_u64();
        if let Ok(mut storage) = self.span_storage.lock() {
            storage.remove(&span_id_u64);
        }
    }

    fn try_close(&self, _id: Id) -> bool {
        // Indicate that we can close the span
        // Clean up happens in drop_span
        true
    }

    fn current_span(&self) -> tracing_core::span::Current {
        // Return current span context based on our thread-local stack
        if let Some(span_id) = self.current_span_id() {
            if let Ok(storage) = self.span_storage.lock() {
                if let Some(span_info) = storage.get(&span_id) {
                    // Create a Current span with the stored metadata
                    return tracing_core::span::Current::new(
                        Id::from_u64(span_id),
                        span_info.metadata,
                    );
                }
            }
        }
        tracing_core::span::Current::none()
    }

    fn register_callsite(&self, _metadata: &'static Metadata<'static>) -> Interest {
        // Always express interest - we handle filtering in our dispatcher
        Interest::always()
    }

    fn max_level_hint(&self) -> Option<tracing_core::LevelFilter> {
        // No filtering at this level - accept all levels
        None
    }
}
