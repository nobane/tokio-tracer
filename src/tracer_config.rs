// src/tracer_config.rs
use serde::{Deserialize, Serialize};

use crate::{Matcher, MatcherSet};

// Main config structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracerConfig {
    pub tabs: Vec<TracerTab>,
}

impl TracerConfig {
    pub fn empty() -> Self {
        Self { tabs: vec![] }
    }
    pub fn default_main_tab() -> Self {
        Self {
            tabs: vec![TracerTab::default()],
        }
    }
    pub fn from_tab(tab: impl Into<TracerTab>) -> Self {
        Self {
            tabs: vec![tab.into()],
        }
    }
    pub fn from_tabs<S: Into<TracerTab>>(tabs: impl IntoIterator<Item = S>) -> Self {
        Self {
            tabs: tabs.into_iter().map(Into::into).collect(),
        }
    }
    /// Add a single tab to the config and return the modified config
    pub fn main_tab(self, matcher_set: impl Into<MatcherSet>) -> Self {
        self.with_tab("Main", matcher_set)
    }
    /// Add a single tab to the config and return the modified config
    pub fn with_tab(mut self, name: impl Into<String>, matcher_set: impl Into<MatcherSet>) -> Self {
        self.tabs
            .push(TracerTab::new(name.into()).with_matcher_set(matcher_set.into()));
        self
    }

    /// Add multiple tabs to the config and return the modified config
    pub fn with_tabs(
        mut self,
        tabs: impl IntoIterator<Item = (impl Into<String>, MatcherSet)>,
    ) -> Self {
        for (name, matcher_set) in tabs {
            self.tabs
                .push(TracerTab::new(name.into()).with_matcher_set(matcher_set));
        }
        self
    }

    /// Add a single tab to the config in-place
    pub fn add_tab(&mut self, name: impl Into<String>, matcher_set: MatcherSet) {
        self.tabs
            .push(TracerTab::new(name.into()).with_matcher_set(matcher_set));
    }

    /// Add multiple tabs to the config in-place
    pub fn add_tabs(&mut self, tabs: impl IntoIterator<Item = (impl Into<String>, MatcherSet)>) {
        for (name, matcher_set) in tabs {
            self.tabs
                .push(TracerTab::new(name.into()).with_matcher_set(matcher_set));
        }
    }
}
// Configuration struct for tabs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracerTab {
    pub name: String,
    pub matcher_set: MatcherSet,
}

impl Default for TracerTab {
    fn default() -> Self {
        Self {
            name: "Main".to_string(),
            matcher_set: MatcherSet::from_matcher(Matcher::debug().all_modules()),
        }
    }
}

impl<S> From<(S, MatcherSet)> for TracerTab
where
    S: Into<String>,
{
    fn from((name, matcher_set): (S, MatcherSet)) -> Self {
        Self {
            name: name.into(),
            matcher_set,
        }
    }
}

impl<S> From<(S, Matcher)> for TracerTab
where
    S: Into<String>,
{
    fn from((name, matcher): (S, Matcher)) -> Self {
        Self {
            name: name.into(),
            matcher_set: MatcherSet::from_matcher(matcher),
        }
    }
}

impl TracerTab {
    pub fn new(name: String) -> Self {
        Self {
            name,
            matcher_set: MatcherSet::empty(),
        }
    }

    pub fn with_matcher_set(mut self, matcher_set: MatcherSet) -> Self {
        self.matcher_set = matcher_set;
        self
    }

    pub fn add_matcher(mut self, matcher: Matcher) -> Self {
        self.matcher_set.add_matcher(matcher);
        self
    }
}
