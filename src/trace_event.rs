// src/trace_event.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tracing::{Event, field::Field};

use std::fmt::Write as _;

pub use tracing::Level as TracingLevel;

use crate::TraceLevel;

pub type TraceEventId = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceData {
    pub id: TraceEventId,
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub level: TraceLevel,
    pub target: String,
    pub name: String,
    pub module_path: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
    pub fields: HashMap<String, String>,
    pub span_name: Option<String>,
    pub span_hierarchy: Option<String>,
}

pub type TraceEvent = Arc<TraceData>;

impl TraceData {
    pub fn new(id: u64, tracing_event: &Event<'_>) -> Self {
        let meta = tracing_event.metadata();

        // Enhanced visitor to capture both formatted event and fields
        let mut visitor = EventMessageVisitor::default();
        tracing_event.record(&mut visitor);

        TraceData {
            id,
            timestamp: chrono::Local::now(),
            level: TraceLevel::from(*meta.level()),
            target: meta.target().to_string(),
            name: meta.name().to_string(),
            module_path: meta.module_path().map(|s| s.to_string()),
            file: meta.file().map(|s| s.to_string()),
            line: meta.line(),
            message: visitor.message,
            fields: visitor.fields,
            span_name: None,      // Will be set by subscriber
            span_hierarchy: None, // Will be set by subscriber
        }
    }

    pub fn into_shared(self) -> Arc<Self> {
        Arc::new(self)
    }

    pub fn ref_count(self: &Arc<Self>) -> usize {
        Arc::strong_count(self)
    }

    pub fn ptr_eq(self: &Arc<Self>, other: &Arc<Self>) -> bool {
        Arc::ptr_eq(self, other)
    }

    pub fn format(&self) -> String {
        let span_info = if let Some(span_name) = &self.span_name {
            format!("[span:{span_name}] ")
        } else {
            String::new()
        };

        format!(
            "{}  {}  {}  {} {}",
            self.timestamp.format("%Y-%m-%d %H:%M:%S"),
            self.level,
            self.message,
            if let Some(module) = &self.module_path {
                format!("[{module}] ")
            } else {
                String::new()
            },
            span_info,
        )
    }

    pub fn format_with_file(&self) -> String {
        let mut formatted = self.format();
        formatted.push_str(&format!(
            " {}:{}",
            self.file.as_deref().unwrap_or("unknown"),
            self.line.unwrap_or(0)
        ));
        formatted
    }

    pub fn format_with_span_hierarchy(&self) -> String {
        let mut formatted = self.format();

        if let Some(hierarchy) = &self.span_hierarchy {
            formatted.push_str(&format!(" [Span Hierarchy: {hierarchy}]"));
        }

        formatted
    }

    pub fn format_with_fields(&self) -> String {
        let mut formatted = self.format();

        if !self.fields.is_empty() {
            formatted.push_str(" {");
            let fields_str = self
                .fields
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", ");
            formatted.push_str(&fields_str);
            formatted.push('}');
        }

        formatted
    }

    pub fn format_full(&self) -> String {
        let mut formatted = self.format_with_file();

        if let Some(hierarchy) = &self.span_hierarchy {
            formatted.push_str(&format!(" [Span Hierarchy: {hierarchy}]"));
        }

        if !self.fields.is_empty() {
            formatted.push_str(" {");
            let fields_str = self
                .fields
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", ");
            formatted.push_str(&fields_str);
            formatted.push('}');
        }

        formatted
    }
}

impl fmt::Display for TraceData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

#[derive(Default)]
pub struct EventMessageVisitor {
    pub fields: HashMap<String, String>,
    pub message: String,
}

const MESSAGE_META: &str = "message";

impl tracing::field::Visit for EventMessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // Special handling for the "message" field which contains the formatted event
        if field.name() == MESSAGE_META {
            if self.message.is_empty() {
                let _ = write!(self.message, "{value:?}");
            }
        } else {
            let mut buffer = String::new();
            let _ = write!(buffer, "{value:?}");
            self.fields.insert(field.name().to_string(), buffer);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == MESSAGE_META {
            if self.message.is_empty() {
                self.message = value.to_string();
            }
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

// Add this to your trace_event.rs file

impl TraceData {
    /// Format with colors using ANSI escape codes
    pub fn format_colored(&self) -> String {
        let span_info = if let Some(span_name) = &self.span_name {
            format!(
                "[span:{}] ",
                ansi_color(span_name, AnsiColor::BrightMagenta)
            )
        } else {
            String::new()
        };

        const HOUR: u8 = 120;
        const MINUTE: u8 = 150;
        const SEC: u8 = 180;

        const HOUR_FG: AnsiColor = AnsiColor::Rgb(HOUR, HOUR, HOUR);
        const MINUTE_FG: AnsiColor = AnsiColor::Rgb(MINUTE, MINUTE, MINUTE);
        const SEC_FG: AnsiColor = AnsiColor::Rgb(SEC, SEC, SEC);

        let hour = ansi_color(&self.timestamp.format("%H").to_string(), HOUR_FG);
        let min = ansi_color(&self.timestamp.format("%M").to_string(), MINUTE_FG);
        let sec = ansi_color(&self.timestamp.format("%S").to_string(), SEC_FG);
        let level = self.format_level_colored();
        let message = ansi_color(&self.message, AnsiColor::White);
        format!("{hour}{min}{sec}  {level}  {message}  {span_info}",)
    }

    /// Format with colors and file info
    pub fn format_colored_with_file(&self) -> String {
        let mut formatted = self.format_colored();
        formatted.push_str(&format!(
            " {}",
            ansi_color(
                &format!(
                    "({}:{})",
                    self.file.as_deref().unwrap_or("unknown"),
                    self.line.unwrap_or(0)
                ),
                AnsiColor::Rgb(90, 90, 90)
            )
        ));
        formatted
    }

    /// Format with colors, file info, and span hierarchy
    pub fn format_colored_with_span_hierarchy(&self) -> String {
        let mut formatted = self.format_colored_with_file();

        if let Some(hierarchy) = &self.span_hierarchy {
            formatted.push_str(&format!(
                " [Span Hierarchy: {}]",
                ansi_color(hierarchy, AnsiColor::BrightMagenta)
            ));
        }

        formatted
    }

    /// Format with colors and fields
    pub fn format_colored_with_fields(&self) -> String {
        let mut formatted = self.format_colored();

        if !self.fields.is_empty() {
            formatted.push_str(" {");
            let fields_str = self
                .fields
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        ansi_color(k, AnsiColor::Cyan),
                        ansi_color(v, AnsiColor::BrightWhite)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            formatted.push_str(&fields_str);
            formatted.push('}');
        }

        formatted
    }

    /// Full colored format with all information
    pub fn format_colored_full(&self) -> String {
        let mut formatted = self.format_colored_with_file();

        if let Some(hierarchy) = &self.span_hierarchy {
            formatted.push_str(&format!(
                " [Span Hierarchy: {}]",
                ansi_color(hierarchy, AnsiColor::BrightMagenta)
            ));
        }

        if !self.fields.is_empty() {
            formatted.push_str(" {");
            let fields_str = self
                .fields
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        ansi_color(k, AnsiColor::Cyan),
                        ansi_color(v, AnsiColor::BrightWhite)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            formatted.push_str(&fields_str);
            formatted.push('}');
        }

        formatted
    }

    /// Format just the level with appropriate colors
    fn format_level_colored(&self) -> String {
        let level_str = match self.level.0 {
            tracing::Level::WARN | tracing::Level::INFO => format!(" {}", self.level),
            _ => self.level.to_string(),
        };

        let color = match self.level.0 {
            tracing::Level::INFO => AnsiColor::Green,
            tracing::Level::DEBUG => AnsiColor::Cyan,
            tracing::Level::WARN => AnsiColor::Yellow,
            tracing::Level::ERROR => AnsiColor::Red,
            tracing::Level::TRACE => AnsiColor::Rgb(128, 128, 128),
        };

        ansi_color(&level_str, color)
    }

    /// Advanced colored format with multiline support
    pub fn format_colored_multiline(&self) -> Vec<String> {
        let mut result = Vec::new();
        let message_parts: Vec<&str> = self.message.split('\n').collect();

        // Create timestamp with colors matching your theme
        let timestamp_colored = format!(
            "{}{}{}",
            ansi_color(
                &self.timestamp.format("%H").to_string(),
                AnsiColor::Rgb(120, 120, 120)
            ),
            ansi_color(
                &self.timestamp.format("%M").to_string(),
                AnsiColor::Rgb(150, 150, 150)
            ),
            ansi_color(
                &self.timestamp.format("%S").to_string(),
                AnsiColor::Rgb(180, 180, 180)
            )
        );

        // Create level with padding and colors
        let level_colored = self.format_level_colored();

        // Generate file/line info if available
        let file_line_info = self.file.as_ref().and_then(|file| {
            self.line
                .as_ref()
                .map(|line| ansi_color(&format!(" ({file}:{line})"), AnsiColor::Rgb(90, 90, 90)))
        });

        // Create the header prefix
        let header_prefix = format!("{timestamp_colored} {level_colored} ");

        if message_parts.len() == 1 {
            // Single line message
            let mut line = format!(
                "{}{}",
                header_prefix,
                ansi_color(message_parts[0], AnsiColor::White)
            );
            if let Some(file_info) = file_line_info {
                line.push_str(&file_info);
            }
            result.push(line);
        } else {
            // Multiline message
            const INDENT_SIZE: usize = 13;
            let indent = " ".repeat(INDENT_SIZE);

            // First line
            result.push(format!(
                "{}{}",
                header_prefix,
                ansi_color(message_parts[0], AnsiColor::White)
            ));

            // Middle lines
            for &line in message_parts.iter().skip(1).take(message_parts.len() - 2) {
                result.push(format!("{}{}", indent, ansi_color(line, AnsiColor::White)));
            }

            // Last line with file info
            if let Some(last_part) = message_parts.last().filter(|_| message_parts.len() > 1) {
                let mut last_line =
                    format!("{}{}", indent, ansi_color(last_part, AnsiColor::White));
                if let Some(file_info) = file_line_info {
                    last_line.push_str(&file_info);
                }
                result.push(last_line);
            }
        }

        // Add span hierarchy if present
        if let Some(hierarchy) = &self.span_hierarchy {
            if let Some(last_line) = result.last_mut() {
                last_line.push_str(&format!(
                    " [Span Hierarchy: {}]",
                    ansi_color(hierarchy, AnsiColor::BrightMagenta)
                ));
            }
        }

        // Add fields if present
        if !self.fields.is_empty() {
            if let Some(last_line) = result.last_mut() {
                last_line.push_str(" {");
                let fields_str = self
                    .fields
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}={}",
                            ansi_color(k, AnsiColor::Cyan),
                            ansi_color(v, AnsiColor::BrightWhite)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                last_line.push_str(&fields_str);
                last_line.push('}');
            }
        }

        result
    }
}

// ANSI color support - add this to the same file or create a separate module

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Rgb(u8, u8, u8),
}

impl AnsiColor {
    fn to_ansi_code(self) -> String {
        match self {
            AnsiColor::Black => "30".to_string(),
            AnsiColor::Red => "31".to_string(),
            AnsiColor::Green => "32".to_string(),
            AnsiColor::Yellow => "33".to_string(),
            AnsiColor::Blue => "34".to_string(),
            AnsiColor::Magenta => "35".to_string(),
            AnsiColor::Cyan => "36".to_string(),
            AnsiColor::White => "37".to_string(),
            AnsiColor::BrightBlack => "90".to_string(),
            AnsiColor::BrightRed => "91".to_string(),
            AnsiColor::BrightGreen => "92".to_string(),
            AnsiColor::BrightYellow => "93".to_string(),
            AnsiColor::BrightBlue => "94".to_string(),
            AnsiColor::BrightMagenta => "95".to_string(),
            AnsiColor::BrightCyan => "96".to_string(),
            AnsiColor::BrightWhite => "97".to_string(),
            AnsiColor::Rgb(r, g, b) => format!("38;2;{r};{g};{b}"),
        }
    }
}

fn ansi_color(text: &str, color: AnsiColor) -> String {
    let code = color.to_ansi_code();
    format!("\x1b[{code}m{text}\x1b[0m")
}
