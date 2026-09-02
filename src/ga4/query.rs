use chrono::{Days, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DateRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl DateRange {
    pub fn new(start: NaiveDate, end: NaiveDate) -> Result<Self> {
        if start > end {
            return Err(AppError::InvalidInput(
                "start date must not be after end date".into(),
            ));
        }
        Ok(Self { start, end })
    }

    pub fn last_complete_days(days: u16) -> Result<Self> {
        if days == 0 {
            return Err(AppError::InvalidInput(
                "date range must contain at least one day".into(),
            ));
        }
        let end = Utc::now().date_naive() - Days::new(1);
        let start = end - Days::new(u64::from(days - 1));
        Self::new(start, end)
    }

    pub fn days(self) -> u64 {
        (self.end - self.start).num_days() as u64 + 1
    }

    pub fn previous(self) -> Self {
        let previous_end = self.start - Days::new(1);
        Self {
            start: previous_end - Days::new(self.days() - 1),
            end: previous_end,
        }
    }

    pub fn shifted(self, periods: i32) -> Self {
        let days = self.days() as i64 * i64::from(periods);
        Self {
            start: self.start + chrono::Duration::days(days),
            end: self.end + chrono::Duration::days(days),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RangePreset {
    Seven,
    TwentyEight,
    Ninety,
}

impl RangePreset {
    pub fn days(self) -> u16 {
        match self {
            Self::Seven => 7,
            Self::TwentyEight => 28,
            Self::Ninety => 90,
        }
    }

    pub fn shorter(self) -> Self {
        match self {
            Self::Ninety => Self::TwentyEight,
            _ => Self::Seven,
        }
    }

    pub fn longer(self) -> Self {
        match self {
            Self::Seven => Self::TwentyEight,
            _ => Self::Ninety,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRunReportsRequest {
    pub requests: Vec<RunReportRequest>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReportRequest {
    pub date_ranges: Vec<ApiDateRange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<Name>,
    pub metrics: Vec<Name>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub order_bys: Vec<OrderBy>,
    pub return_property_quota: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRealtimeReportRequest {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<Name>,
    pub metrics: Vec<Name>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub order_bys: Vec<OrderBy>,
    pub return_property_quota: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiDateRange {
    pub start_date: String,
    pub end_date: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct Name {
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBy {
    pub desc: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<MetricOrder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimension: Option<DimensionOrder>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricOrder {
    pub metric_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DimensionOrder {
    pub dimension_name: String,
    pub order_type: String,
}

impl Name {
    fn new(name: &str) -> Self {
        Self { name: name.into() }
    }
}

pub fn overview_request(range: DateRange, comparison: bool) -> BatchRunReportsRequest {
    let mut ranges = vec![api_range("current", range)];
    if comparison {
        ranges.push(api_range("previous", range.previous()));
    }
    BatchRunReportsRequest {
        requests: vec![
            RunReportRequest {
                date_ranges: ranges.clone(),
                dimensions: vec![],
                metrics: names(&["activeUsers", "sessions", "engagementRate", "keyEvents"]),
                limit: None,
                order_bys: vec![],
                return_property_quota: true,
            },
            RunReportRequest {
                date_ranges: ranges,
                dimensions: names(&["date"]),
                metrics: names(&["activeUsers"]),
                limit: None,
                order_bys: vec![dimension_order("date", false)],
                return_property_quota: true,
            },
            RunReportRequest {
                date_ranges: vec![api_range("current", range)],
                dimensions: names(&["sessionPrimaryChannelGroup"]),
                metrics: names(&["activeUsers", "sessions", "engagementRate"]),
                limit: Some("10".into()),
                order_bys: vec![metric_order("sessions", true)],
                return_property_quota: true,
            },
        ],
    }
}

pub fn realtime_requests() -> [RunRealtimeReportRequest; 4] {
    [
        realtime(&[], &["activeUsers"], None, None),
        realtime(&["minutesAgo"], &["activeUsers"], Some(30), None),
        realtime(
            &["unifiedScreenName"],
            &["activeUsers", "screenPageViews"],
            Some(10),
            Some("activeUsers"),
        ),
        realtime(
            &["eventName"],
            &["eventCount"],
            Some(10),
            Some("eventCount"),
        ),
    ]
}

fn realtime(
    dimensions: &[&str],
    metrics: &[&str],
    limit: Option<u16>,
    order_metric: Option<&str>,
) -> RunRealtimeReportRequest {
    RunRealtimeReportRequest {
        dimensions: names(dimensions),
        metrics: names(metrics),
        limit: limit.map(|v| v.to_string()),
        order_bys: order_metric
            .map(|name| vec![metric_order(name, true)])
            .unwrap_or_default(),
        return_property_quota: true,
    }
}

fn names(values: &[&str]) -> Vec<Name> {
    values.iter().map(|value| Name::new(value)).collect()
}
fn api_range(name: &str, range: DateRange) -> ApiDateRange {
    ApiDateRange {
        start_date: range.start.to_string(),
        end_date: range.end.to_string(),
        name: name.into(),
    }
}
fn metric_order(name: &str, desc: bool) -> OrderBy {
    OrderBy {
        desc,
        metric: Some(MetricOrder {
            metric_name: name.into(),
        }),
        dimension: None,
    }
}
fn dimension_order(name: &str, desc: bool) -> OrderBy {
    OrderBy {
        desc,
        metric: None,
        dimension: Some(DimensionOrder {
            dimension_name: name.into(),
            order_type: "ALPHANUMERIC".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_range_is_equal_and_adjacent() {
        let range = DateRange::new(
            NaiveDate::from_ymd_opt(2026, 8, 6).unwrap(),
            NaiveDate::from_ymd_opt(2026, 9, 2).unwrap(),
        )
        .unwrap();
        let previous = range.previous();
        assert_eq!(previous.days(), 28);
        assert_eq!(previous.end, NaiveDate::from_ymd_opt(2026, 8, 5).unwrap());
    }

    #[test]
    fn overview_has_three_batched_reports() {
        let range = DateRange::new(
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 28).unwrap(),
        )
        .unwrap();
        let value = serde_json::to_value(overview_request(range, true)).unwrap();
        assert_eq!(value["requests"].as_array().unwrap().len(), 3);
        assert_eq!(
            value["requests"][0]["dateRanges"].as_array().unwrap().len(),
            2
        );
    }
}
