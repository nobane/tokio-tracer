// src/lib.rs
mod tracer;
pub use tracer::Tracer;
use tracer::{DroppedEventCallback, EventCallback, SilencedEventCallback};

mod trace_event;
pub use trace_event::{TraceData, TraceEvent, TraceEventId, TracingLevel};

mod trace_matcher;
pub use trace_matcher::{Matcher, MatcherSet, TraceLevel, matches};

mod tracing_dispatcher;
use tracing_dispatcher::{DispatcherCommand, ResultSender, TraceCounters, TracingDispatcher};

mod tracer_config;
pub use tracer_config::{TracerConfig, TracerTab};

mod tracing_subscriber;
pub use tracing_subscriber::TracingSubscriber;
