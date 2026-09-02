use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropertySummary {
    pub account: String,
    pub account_name: String,
    pub property: String,
    pub property_name: String,
}

impl PropertySummary {
    pub fn id(&self) -> &str {
        self.property
            .strip_prefix("properties/")
            .unwrap_or(&self.property)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricValue {
    pub current: f64,
    pub previous: Option<f64>,
}

impl MetricValue {
    pub fn delta_percent(&self) -> Option<f64> {
        match self.previous {
            Some(previous) if previous != 0.0 => Some((self.current - previous) / previous * 100.0),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SummaryMetrics {
    pub active_users: MetricValue,
    pub sessions: MetricValue,
    pub engagement_rate: MetricValue,
    pub key_events: MetricValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrendPoint {
    pub date: NaiveDate,
    pub active_users: f64,
    pub comparison: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelRow {
    pub channel: String,
    pub active_users: f64,
    pub sessions: f64,
    pub engagement_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QuotaSnapshot {
    pub tokens_per_day_remaining: Option<i64>,
    pub tokens_per_hour_remaining: Option<i64>,
    pub concurrent_requests_remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OverviewReport {
    pub property_id: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub summary: SummaryMetrics,
    pub trend: Vec<TrendPoint>,
    pub channels: Vec<ChannelRow>,
    pub quota: QuotaSnapshot,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimePoint {
    pub minutes_ago: u16,
    pub active_users: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RankedRow {
    pub name: String,
    pub primary: f64,
    pub secondary: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealtimeReport {
    pub property_id: String,
    pub active_users: f64,
    pub trend: Vec<RealtimePoint>,
    pub pages: Vec<RankedRow>,
    pub events: Vec<RankedRow>,
    pub quota: QuotaSnapshot,
    pub fetched_at: DateTime<Utc>,
}

pub fn format_number(value: f64) -> String {
    if value.abs() >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value.abs() >= 10_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else if value.fract().abs() < f64::EPSILON {
        format_integer(value as i64)
    } else {
        format!("{value:.1}")
    }
}

fn format_integer(value: i64) -> String {
    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let mut output = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            output.push(',');
        }
        output.push(character);
    }
    if negative {
        format!("-{output}")
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_handles_zero_baseline() {
        assert_eq!(
            MetricValue {
                current: 4.0,
                previous: Some(0.0)
            }
            .delta_percent(),
            None
        );
        assert_eq!(
            MetricValue {
                current: 15.0,
                previous: Some(10.0)
            }
            .delta_percent(),
            Some(50.0)
        );
    }

    #[test]
    fn numbers_are_compact_but_readable() {
        assert_eq!(format_number(9999.0), "9,999");
        assert_eq!(format_number(18429.0), "18.4K");
        assert_eq!(format_number(1_250_000.0), "1.2M");
    }
}
