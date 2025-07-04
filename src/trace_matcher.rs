// src/trace_matcher.rs
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::Level;

use crate::TraceData;

// Helper function for pattern matching
pub fn matches(pattern: &str, value: &str) -> bool {
    // Simple glob matching (* as wildcard)
    let pattern = pattern.replace("*", ".*");
    if let Ok(re) = Regex::new(&format!("^{pattern}$")) {
        re.is_match(value)
    } else {
        false
    }
}

// Define TraceLevel for serialization
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceLevel(pub Level);

impl TraceLevel {
    pub fn is_trace(&self) -> bool {
        self.0 == Level::TRACE
    }
    pub fn is_error(&self) -> bool {
        self.0 == Level::ERROR
    }
    pub fn is_warn(&self) -> bool {
        self.0 == Level::WARN
    }
    pub fn is_info(&self) -> bool {
        self.0 == Level::INFO
    }
    pub fn is_debug(&self) -> bool {
        self.0 == Level::DEBUG
    }
}

impl Serialize for TraceLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self.0 {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN",
            Level::INFO => "INFO",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "TRACE",
        };
        serializer.serialize_str(s)
    }
}

impl<'de> Deserialize<'de> for TraceLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let level = match s.to_uppercase().as_str() {
            "ERROR" => Level::ERROR,
            "WARN" => Level::WARN,
            "INFO" => Level::INFO,
            "DEBUG" => Level::DEBUG,
            "TRACE" => Level::TRACE,
            _ => {
                return Err(serde::de::Error::custom(format!(
                    "invalid level filter: {s}"
                )));
            }
        };
        Ok(TraceLevel(level))
    }
}

impl std::hash::Hash for TraceLevel {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Using the integer representation of the filter
        let value = match self.0 {
            Level::ERROR => 1,
            Level::WARN => 2,
            Level::INFO => 3,
            Level::DEBUG => 4,
            Level::TRACE => 5,
        };
        value.hash(state);
    }
}

impl Eq for TraceLevel {}

impl std::fmt::Display for TraceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Level::ERROR => write!(f, "ERROR"),
            Level::WARN => write!(f, "WARN"),
            Level::INFO => write!(f, "INFO"),
            Level::DEBUG => write!(f, "DEBUG"),
            Level::TRACE => write!(f, "TRACE"),
        }
    }
}

impl From<Level> for TraceLevel {
    fn from(filter: Level) -> Self {
        TraceLevel(filter)
    }
}

impl From<TraceLevel> for Level {
    fn from(level: TraceLevel) -> Self {
        level.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Matcher {
    pub level: TraceLevel,
    pub include: bool,
    /// pub has_module_wildcard: bool, // TODO: Optimize
    pub module_patterns: Vec<String>,
    pub file_patterns: Vec<String>,
    pub span_patterns: Vec<String>,
    pub target_patterns: Vec<String>,
}

impl std::hash::Hash for Matcher {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.include.hash(state);
        self.level.hash(state);
        self.module_patterns.hash(state);
        self.file_patterns.hash(state);
        self.span_patterns.hash(state);
        self.target_patterns.hash(state);
    }
}

impl Default for Matcher {
    fn default() -> Self {
        Self {
            level: TraceLevel(Level::DEBUG),
            include: true,
            module_patterns: vec!["*".to_string()],
            file_patterns: vec![],
            span_patterns: vec![],
            target_patterns: vec![],
        }
    }
}

impl Eq for Matcher {}

impl PartialEq for Matcher {
    fn eq(&self, other: &Self) -> bool {
        self.level == other.level
            && self.module_patterns == other.module_patterns
            && self.file_patterns == other.file_patterns
            && self.span_patterns == other.span_patterns
            && self.target_patterns == other.target_patterns
    }
}

// Builder methods for TraceFilter
impl Matcher {
    pub fn new(level: impl Into<TraceLevel>) -> Self {
        Self {
            level: level.into(),
            include: true,
            module_patterns: vec![],
            file_patterns: vec![],
            span_patterns: vec![],
            target_patterns: vec![],
        }
    }

    pub fn trace() -> Self {
        Self::new(Level::TRACE)
    }

    pub fn debug() -> Self {
        Self::new(Level::DEBUG)
    }

    pub fn info() -> Self {
        Self::new(Level::INFO)
    }

    pub fn warn() -> Self {
        Self::new(Level::WARN)
    }

    pub fn error() -> Self {
        Self::new(Level::ERROR)
    }

    // Set inclusion/exclusion
    pub fn include(mut self) -> Self {
        self.include = true;
        self
    }

    pub fn exclude(mut self) -> Self {
        self.include = false;
        self
    }

    pub fn module_patterns(
        mut self,
        patterns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.module_patterns = patterns.into_iter().map(Into::<String>::into).collect();
        self
    }

    pub fn module_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.module_patterns.push(pattern.into());
        self
    }

    pub fn extend_module_patterns(
        mut self,
        patterns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.module_patterns
            .extend(patterns.into_iter().map(Into::<String>::into));
        self
    }

    pub fn file_patterns(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.file_patterns = patterns.into_iter().map(Into::<String>::into).collect();
        self
    }

    pub fn file_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.file_patterns.push(pattern.into());
        self
    }

    pub fn extend_file_patterns(
        mut self,
        patterns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.file_patterns
            .extend(patterns.into_iter().map(Into::<String>::into));
        self
    }

    pub fn span_patterns(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.span_patterns = patterns.into_iter().map(Into::<String>::into).collect();
        self
    }

    pub fn span_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.span_patterns.push(pattern.into());
        self
    }

    pub fn extend_span_patterns(
        mut self,
        patterns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.span_patterns
            .extend(patterns.into_iter().map(Into::<String>::into));
        self
    }

    pub fn target_patterns(
        mut self,
        patterns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.target_patterns = patterns.into_iter().map(Into::<String>::into).collect();
        self
    }

    pub fn target_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.target_patterns.push(pattern.into());
        self
    }

    pub fn extend_target_patterns(
        mut self,
        patterns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.target_patterns
            .extend(patterns.into_iter().map(Into::<String>::into));
        self
    }

    // Shorthand for common patterns
    pub fn all_modules(mut self) -> Self {
        self.module_patterns.push("*".to_string());
        self
    }

    pub fn into_matcher_set(self) -> MatcherSet {
        MatcherSet::from_matcher(self)
    }

    pub fn matches(&self, event: &TraceData) -> bool {
        // Check level first
        match self.level.0 {
            Level::ERROR => {
                if event.level.0 != Level::ERROR {
                    return false;
                }
            }
            Level::WARN => {
                if !matches!(event.level.0, Level::ERROR | Level::WARN) {
                    return false;
                }
            }
            Level::INFO => {
                if !matches!(event.level.0, Level::ERROR | Level::WARN | Level::INFO) {
                    return false;
                }
            }
            Level::DEBUG => {
                if !matches!(
                    event.level.0,
                    Level::ERROR | Level::WARN | Level::INFO | Level::DEBUG
                ) {
                    return false;
                }
            }
            Level::TRACE => {} // All levels pass
        }

        // Check module path
        if let Some(module_path) = &event.module_path {
            // If we have include patterns, at least one must match
            if !self.module_patterns.is_empty() {
                let mut module_matched = false;
                for pattern in &self.module_patterns {
                    if matches(pattern, module_path) {
                        module_matched = true;
                        break;
                    }
                }
                if !module_matched {
                    return false;
                }
            }
        } else if !self.module_patterns.is_empty() {
            // Special case: if there's a wildcard pattern, allow no-module events
            let has_wildcard = self.module_patterns.iter().any(|p| p == "*");
            if !has_wildcard {
                // If we require a specific module pattern but there's no module path, exclude
                return false;
            }
        }

        // Check file path
        // If we have include patterns, at least one must match
        if !self.file_patterns.is_empty() {
            let mut file_matched = false;
            if let Some(file) = &event.file {
                for pattern in &self.file_patterns {
                    if matches(pattern, file) {
                        file_matched = true;
                        break;
                    }
                }
            }
            if !file_matched {
                return false;
            }
        }

        // Check span name
        if !self.span_patterns.is_empty() {
            let mut span_matched = false;
            if let Some(span_name) = &event.span_name {
                for pattern in &self.span_patterns {
                    if matches(pattern, span_name) {
                        span_matched = true;
                        break;
                    }
                }
            }
            if !span_matched {
                return false;
            }
        }

        // Check target
        if !self.target_patterns.is_empty() {
            let mut target_matched = false;
            for pattern in &self.target_patterns {
                if matches(pattern, &event.target) {
                    target_matched = true;
                    break;
                }
            }
            if !target_matched {
                return false;
            }
        }

        true
    }
}

impl From<Matcher> for MatcherSet {
    fn from(val: Matcher) -> Self {
        val.into_matcher_set()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatcherSet {
    matchers: std::collections::HashSet<Matcher>,
}

// Add builder methods for TraceFilterSet as well
impl MatcherSet {
    pub fn empty() -> Self {
        Self {
            matchers: std::collections::HashSet::new(),
        }
    }

    pub fn from_matcher(matcher: Matcher) -> Self {
        let mut filter = Self::empty();
        filter.matchers.insert(matcher);
        filter
    }

    pub fn from_matchers(matchers: impl IntoIterator<Item = Matcher>) -> Self {
        let mut filter = Self::empty();
        for matcher in matchers {
            filter.matchers.insert(matcher);
        }
        filter
    }

    pub fn with_matcher(mut self, filter: Matcher) -> Self {
        self.matchers.replace(filter);
        self
    }

    pub fn add_matcher(&mut self, filter: Matcher) {
        self.matchers.replace(filter);
    }

    pub fn remove_matcher(&mut self, filter: &Matcher) -> bool {
        self.matchers.remove(filter)
    }

    pub fn clear_matchers(&mut self) {
        self.matchers.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.matchers.is_empty()
    }

    pub fn iter_matchers(&self) -> Vec<&Matcher> {
        self.matchers.iter().collect()
    }
}
