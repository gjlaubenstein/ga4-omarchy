use chrono::Utc;

use crate::{
    config::Config,
    ga4::{DateRange, OverviewReport, PropertySummary, RangePreset, RealtimeReport},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Overview,
    Realtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Chart,
    Table,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    Help,
    Properties,
    DateInput,
    Detail,
}

#[derive(Debug)]
pub enum Message {
    OverviewLoaded {
        generation: u64,
        result: crate::error::Result<OverviewReport>,
    },
    RealtimeLoaded {
        generation: u64,
        result: crate::error::Result<RealtimeReport>,
    },
    PropertiesLoaded(crate::error::Result<Vec<PropertySummary>>),
}

pub struct AppState {
    pub config: Config,
    pub view: View,
    pub focus: Focus,
    pub overlay: Option<Overlay>,
    pub preset: RangePreset,
    pub range: DateRange,
    pub overview: Option<OverviewReport>,
    pub realtime: Option<RealtimeReport>,
    pub properties: Vec<PropertySummary>,
    pub selected_row: usize,
    pub property_row: usize,
    pub property_filter: String,
    pub filter: String,
    pub editing_filter: bool,
    pub date_input: String,
    pub loading: bool,
    pub stale: bool,
    pub status: String,
    pub error: Option<String>,
    pub generation: u64,
    pub quit: bool,
}

impl AppState {
    pub fn new(config: Config) -> crate::error::Result<Self> {
        let preset = match config.default_range_days {
            7 => RangePreset::Seven,
            90 => RangePreset::Ninety,
            _ => RangePreset::TwentyEight,
        };
        Ok(Self {
            range: DateRange::last_complete_days(preset.days())?,
            config,
            preset,
            view: View::Overview,
            focus: Focus::Chart,
            overlay: None,
            overview: None,
            realtime: None,
            properties: Vec::new(),
            selected_row: 0,
            property_row: 0,
            property_filter: String::new(),
            filter: String::new(),
            editing_filter: false,
            date_input: String::new(),
            loading: false,
            stale: false,
            status: "ready".into(),
            error: None,
            generation: 0,
            quit: false,
        })
    }

    pub fn selected_property(&self) -> Option<&str> {
        self.config.selected_property.as_deref()
    }

    pub fn begin_request(&mut self, status: &str) -> u64 {
        self.generation += 1;
        self.loading = true;
        self.error = None;
        self.status = status.into();
        self.generation
    }

    pub fn apply_message(&mut self, message: Message) {
        match message {
            Message::OverviewLoaded { generation, result } if generation == self.generation => {
                self.loading = false;
                match result {
                    Ok(report) => {
                        self.overview = Some(report);
                        self.stale = false;
                        self.status = "updated just now".into();
                        self.error = None;
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        self.status = if self.overview.is_some() {
                            "showing cached data".into()
                        } else {
                            "load failed".into()
                        };
                    }
                }
            }
            Message::RealtimeLoaded { generation, result } if generation == self.generation => {
                self.loading = false;
                match result {
                    Ok(report) => {
                        self.realtime = Some(report);
                        self.status = "live · updated just now".into();
                        self.error = None;
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        self.status = "realtime refresh failed".into();
                    }
                }
            }
            Message::PropertiesLoaded(result) => {
                self.loading = false;
                match result {
                    Ok(properties) => {
                        self.properties = properties;
                        self.status = format!("{} properties", self.properties.len());
                        if self.config.selected_property.is_none() {
                            self.overlay = Some(Overlay::Properties);
                        }
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            _ => {}
        }
    }

    pub fn age_label(&self) -> String {
        let fetched = match self.view {
            View::Overview => self.overview.as_ref().map(|r| r.fetched_at),
            View::Realtime => self.realtime.as_ref().map(|r| r.fetched_at),
        };
        fetched
            .map(|time| {
                let minutes = Utc::now().signed_duration_since(time).num_minutes().max(0);
                if minutes == 0 {
                    "just now".into()
                } else {
                    format!("{minutes}m ago")
                }
            })
            .unwrap_or_else(|| "never".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsolete_messages_are_ignored() {
        let mut app = AppState::new(Config::default()).unwrap();
        app.generation = 2;
        app.loading = true;
        app.apply_message(Message::OverviewLoaded {
            generation: 1,
            result: Err(crate::error::AppError::Offline("old".into())),
        });
        assert!(app.loading);
        assert!(app.error.is_none());
    }
}
