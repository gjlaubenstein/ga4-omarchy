use std::time::Duration;

use chrono::{NaiveDate, Utc};
use reqwest::{header::RETRY_AFTER, StatusCode};
use serde::Deserialize;

use crate::{
    auth::AuthService,
    error::{AppError, Result},
    ga4::{
        overview_request, realtime_requests, ChannelRow, DateRange, MetricValue, OverviewReport,
        PropertySummary, QuotaSnapshot, RankedRow, RealtimePoint, RealtimeReport, SummaryMetrics,
        TrendPoint,
    },
};

const DATA_API: &str = "https://analyticsdata.googleapis.com/v1beta";
const ADMIN_API: &str = "https://analyticsadmin.googleapis.com/v1beta";

#[derive(Clone)]
pub struct Ga4Client {
    http: reqwest::Client,
    auth: AuthService,
    data_api: String,
    admin_api: String,
}

impl Ga4Client {
    pub fn new(auth: AuthService) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("ga4-omarchy/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| AppError::Other(error.to_string()))?;
        Ok(Self {
            http,
            auth,
            data_api: DATA_API.into(),
            admin_api: ADMIN_API.into(),
        })
    }

    #[cfg(test)]
    pub fn with_endpoints(auth: AuthService, data_api: String, admin_api: String) -> Result<Self> {
        let mut client = Self::new(auth)?;
        client.data_api = data_api;
        client.admin_api = admin_api;
        Ok(client)
    }

    pub async fn properties(&self) -> Result<Vec<PropertySummary>> {
        let token = self.auth.access_token().await?;
        let mut page_token: Option<String> = None;
        let mut properties = Vec::new();
        loop {
            let mut request = self
                .http
                .get(format!("{}/accountSummaries?pageSize=200", self.admin_api))
                .bearer_auth(&token);
            if let Some(page) = &page_token {
                request = request.query(&[("pageToken", page)]);
            }
            let response = request.send().await.map_err(map_transport)?;
            let response = ensure_success(response).await?;
            let page: AccountSummariesResponse = response
                .json()
                .await
                .map_err(|error| AppError::Server(error.to_string()))?;
            for account in page.account_summaries {
                for property in account.property_summaries {
                    properties.push(PropertySummary {
                        account: account.account.clone(),
                        account_name: account.display_name.clone(),
                        property: property.property,
                        property_name: property.display_name,
                    });
                }
            }
            match page.next_page_token.filter(|token| !token.is_empty()) {
                Some(next) => page_token = Some(next),
                None => break,
            }
        }
        properties.sort_by(|a, b| {
            a.account_name
                .cmp(&b.account_name)
                .then(a.property_name.cmp(&b.property_name))
        });
        Ok(properties)
    }

    pub async fn overview(
        &self,
        property_id: &str,
        range: DateRange,
        comparison: bool,
    ) -> Result<OverviewReport> {
        validate_property(property_id)?;
        let token = self.auth.access_token().await?;
        let response = self
            .http
            .post(format!(
                "{}/properties/{}:batchRunReports",
                self.data_api, property_id
            ))
            .bearer_auth(token)
            .json(&overview_request(range, comparison))
            .send()
            .await
            .map_err(map_transport)?;
        let raw: BatchResponse = ensure_success(response)
            .await?
            .json()
            .await
            .map_err(|error| AppError::Server(error.to_string()))?;
        normalize_overview(property_id, range, raw)
    }

    pub async fn realtime(&self, property_id: &str) -> Result<RealtimeReport> {
        validate_property(property_id)?;
        let token = self.auth.access_token().await?;
        let mut reports = Vec::new();
        for request in realtime_requests() {
            let response = self
                .http
                .post(format!(
                    "{}/properties/{}:runRealtimeReport",
                    self.data_api, property_id
                ))
                .bearer_auth(&token)
                .json(&request)
                .send()
                .await
                .map_err(map_transport)?;
            reports.push(
                ensure_success(response)
                    .await?
                    .json::<RawReport>()
                    .await
                    .map_err(|error| AppError::Server(error.to_string()))?,
            );
        }
        normalize_realtime(property_id, reports)
    }
}

fn validate_property(property_id: &str) -> Result<()> {
    if property_id.is_empty() || !property_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::InvalidInput(
            "property ID must contain only digits".into(),
        ));
    }
    Ok(())
}

async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let retry = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .map(Duration::from_secs);
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<GoogleError>(&body)
        .ok()
        .map(|value| value.error.message)
        .unwrap_or_else(|| status.to_string());
    match status {
        StatusCode::UNAUTHORIZED => Err(AppError::Unauthenticated),
        StatusCode::FORBIDDEN
            if message.to_ascii_lowercase().contains("has not been used")
                || message.to_ascii_lowercase().contains("disabled") =>
        {
            Err(AppError::ApiDisabled)
        }
        StatusCode::FORBIDDEN => Err(AppError::PermissionDenied),
        StatusCode::BAD_REQUEST => Err(AppError::IncompatibleQuery(message)),
        StatusCode::TOO_MANY_REQUESTS => Err(AppError::QuotaLimited(retry)),
        status if status.is_server_error() => Err(AppError::Server(message)),
        _ => Err(AppError::Other(message)),
    }
}

fn map_transport(error: reqwest::Error) -> AppError {
    if error.is_connect() || error.is_timeout() {
        AppError::Offline(error.to_string())
    } else {
        AppError::Server(error.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountSummariesResponse {
    #[serde(default)]
    account_summaries: Vec<AccountSummaryRaw>,
    next_page_token: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountSummaryRaw {
    account: String,
    display_name: String,
    #[serde(default)]
    property_summaries: Vec<PropertySummaryRaw>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PropertySummaryRaw {
    property: String,
    display_name: String,
}
#[derive(Deserialize)]
struct GoogleError {
    error: GoogleErrorBody,
}
#[derive(Deserialize)]
struct GoogleErrorBody {
    message: String,
}

#[derive(Debug, Default, Deserialize)]
struct BatchResponse {
    #[serde(default)]
    reports: Vec<RawReport>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReport {
    #[serde(default)]
    dimension_headers: Vec<Header>,
    #[serde(default)]
    rows: Vec<RawRow>,
    property_quota: Option<RawQuota>,
}

#[derive(Debug, Deserialize)]
struct Header {
    name: String,
}
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRow {
    #[serde(default)]
    dimension_values: Vec<RawValue>,
    #[serde(default)]
    metric_values: Vec<RawValue>,
}
#[derive(Debug, Deserialize)]
struct RawValue {
    value: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawQuota {
    tokens_per_day: Option<QuotaStatus>,
    tokens_per_hour: Option<QuotaStatus>,
    concurrent_requests: Option<QuotaStatus>,
}
#[derive(Debug, Deserialize)]
struct QuotaStatus {
    remaining: i64,
}

fn normalize_overview(
    property_id: &str,
    range: DateRange,
    mut batch: BatchResponse,
) -> Result<OverviewReport> {
    if batch.reports.len() < 3 {
        return Err(AppError::Server(
            "batch response did not include all overview reports".into(),
        ));
    }
    let channels_report = batch.reports.pop().unwrap();
    let trend_report = batch.reports.pop().unwrap();
    let summary_report = batch.reports.pop().unwrap();
    let summary_range_index = header_index(&summary_report.dimension_headers, "dateRange");
    let summary_values: Vec<Vec<f64>> = summary_report.rows.iter().map(metrics).collect();
    let by_range = |expected: &str| {
        summary_report
            .rows
            .iter()
            .position(|row| {
                summary_range_index
                    .and_then(|index| row.dimension_values.get(index))
                    .map(|value| value.value.as_str())
                    == Some(expected)
            })
            .and_then(|index| summary_values.get(index))
    };
    let current = by_range("0")
        .or_else(|| summary_values.first())
        .cloned()
        .unwrap_or_else(|| vec![0.0; 4]);
    let previous = by_range("1").or_else(|| summary_values.get(1));
    let metric = |index| MetricValue {
        current: *current.get(index).unwrap_or(&0.0),
        previous: previous.and_then(|values| values.get(index)).copied(),
    };
    let summary = SummaryMetrics {
        active_users: metric(0),
        sessions: metric(1),
        engagement_rate: metric(2),
        key_events: metric(3),
    };

    let date_index = header_index(&trend_report.dimension_headers, "date").unwrap_or(0);
    let range_index = header_index(&trend_report.dimension_headers, "dateRange");
    let mut trend = Vec::new();
    for row in &trend_report.rows {
        let Some(date) = row
            .dimension_values
            .get(date_index)
            .and_then(|v| NaiveDate::parse_from_str(&v.value, "%Y%m%d").ok())
        else {
            continue;
        };
        let comparison = range_index
            .and_then(|index| row.dimension_values.get(index))
            .map(|v| v.value == "1")
            .unwrap_or(date < range.start);
        trend.push(TrendPoint {
            date,
            active_users: metric_at(row, 0),
            comparison,
        });
    }
    trend.sort_by_key(|point| (point.comparison, point.date));

    let channels = channels_report
        .rows
        .iter()
        .map(|row| ChannelRow {
            channel: row
                .dimension_values
                .first()
                .map(|v| v.value.clone())
                .unwrap_or_else(|| "(not set)".into()),
            active_users: metric_at(row, 0),
            sessions: metric_at(row, 1),
            engagement_rate: metric_at(row, 2),
        })
        .collect();
    let quota = quota_from([&summary_report, &trend_report, &channels_report]);
    Ok(OverviewReport {
        property_id: property_id.into(),
        start: range.start,
        end: range.end,
        summary,
        trend,
        channels,
        quota,
        fetched_at: Utc::now(),
    })
}

fn normalize_realtime(property_id: &str, reports: Vec<RawReport>) -> Result<RealtimeReport> {
    if reports.len() != 4 {
        return Err(AppError::Server(
            "realtime response did not include all reports".into(),
        ));
    }
    let active_users = reports[0]
        .rows
        .first()
        .map(|row| metric_at(row, 0))
        .unwrap_or(0.0);
    let mut trend: Vec<_> = reports[1]
        .rows
        .iter()
        .filter_map(|row| {
            row.dimension_values
                .first()?
                .value
                .parse()
                .ok()
                .map(|minutes_ago| RealtimePoint {
                    minutes_ago,
                    active_users: metric_at(row, 0),
                })
        })
        .collect();
    trend.sort_by_key(|point| std::cmp::Reverse(point.minutes_ago));
    let ranked = |report: &RawReport| {
        report
            .rows
            .iter()
            .map(|row| RankedRow {
                name: row
                    .dimension_values
                    .first()
                    .map(|v| v.value.clone())
                    .unwrap_or_else(|| "(not set)".into()),
                primary: metric_at(row, 0),
                secondary: metric_at(row, 1),
            })
            .collect()
    };
    let quota = quota_from(reports.iter());
    Ok(RealtimeReport {
        property_id: property_id.into(),
        active_users,
        trend,
        pages: ranked(&reports[2]),
        events: ranked(&reports[3]),
        quota,
        fetched_at: Utc::now(),
    })
}

fn header_index(headers: &[Header], name: &str) -> Option<usize> {
    headers.iter().position(|header| header.name == name)
}
fn metric_at(row: &RawRow, index: usize) -> f64 {
    row.metric_values
        .get(index)
        .and_then(|v| v.value.parse().ok())
        .unwrap_or(0.0)
}
fn metrics(row: &RawRow) -> Vec<f64> {
    row.metric_values
        .iter()
        .map(|value| value.value.parse().unwrap_or(0.0))
        .collect()
}
fn quota_from<'a>(reports: impl IntoIterator<Item = &'a RawReport>) -> QuotaSnapshot {
    reports
        .into_iter()
        .filter_map(|report| report.property_quota.as_ref())
        .fold(QuotaSnapshot::default(), |mut output, quota| {
            output.tokens_per_day_remaining = quota
                .tokens_per_day
                .as_ref()
                .map(|value| value.remaining)
                .or(output.tokens_per_day_remaining);
            output.tokens_per_hour_remaining = quota
                .tokens_per_hour
                .as_ref()
                .map(|value| value.remaining)
                .or(output.tokens_per_hour_remaining);
            output.concurrent_requests_remaining = quota
                .concurrent_requests
                .as_ref()
                .map(|value| value.remaining)
                .or(output.concurrent_requests_remaining);
            output
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use httpmock::prelude::*;
    use std::time::Duration;

    #[test]
    fn overview_fixture_is_normalized_by_date_range_index() {
        let raw: BatchResponse = serde_json::from_value(serde_json::json!({
            "reports": [
                {"dimensionHeaders":[{"name":"dateRange"}],"rows":[
                    {"dimensionValues":[{"value":"1"}],"metricValues":[{"value":"80"},{"value":"100"},{"value":"0.5"},{"value":"4"}]},
                    {"dimensionValues":[{"value":"0"}],"metricValues":[{"value":"120"},{"value":"150"},{"value":"0.6"},{"value":"8"}]}
                ]},
                {"dimensionHeaders":[{"name":"date"},{"name":"dateRange"}],"rows":[
                    {"dimensionValues":[{"value":"20260801"},{"value":"0"}],"metricValues":[{"value":"12"}]},
                    {"dimensionValues":[{"value":"20260701"},{"value":"1"}],"metricValues":[{"value":"8"}]}
                ]},
                {"dimensionHeaders":[{"name":"sessionPrimaryChannelGroup"}],"rows":[
                    {"dimensionValues":[{"value":"Organic Search"}],"metricValues":[{"value":"50"},{"value":"70"},{"value":"0.7"}]}
                ],"propertyQuota":{"tokensPerDay":{"remaining":199990}}}
            ]
        })).unwrap();
        let range = DateRange::new(
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 28).unwrap(),
        )
        .unwrap();
        let report = normalize_overview("123", range, raw).unwrap();
        assert_eq!(report.summary.active_users.current, 120.0);
        assert_eq!(report.summary.active_users.previous, Some(80.0));
        assert!(report.trend.iter().any(|point| point.comparison));
        assert_eq!(report.channels[0].sessions, 70.0);
        assert_eq!(report.quota.tokens_per_day_remaining, Some(199990));
    }

    #[tokio::test]
    async fn rate_limit_preserves_retry_after() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(429)
                    .header("retry-after", "12")
                    .json_body(serde_json::json!({"error":{"message":"quota exhausted"}}));
            })
            .await;
        let response = reqwest::get(server.url("/")).await.unwrap();
        let error = ensure_success(response).await.unwrap_err();
        assert!(
            matches!(error, AppError::QuotaLimited(Some(value)) if value == Duration::from_secs(12))
        );
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn disabled_api_is_distinct_from_permission_denied() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(403).json_body(
                    serde_json::json!({"error":{"message":"API has not been used or is disabled"}}),
                );
            })
            .await;
        let response = reqwest::get(server.url("/")).await.unwrap();
        assert!(matches!(
            ensure_success(response).await.unwrap_err(),
            AppError::ApiDisabled
        ));
    }
}
