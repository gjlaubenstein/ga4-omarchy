use std::io::Write;

use crate::{
    error::{AppError, Result},
    ga4::OverviewReport,
};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportFormat {
    Csv,
    Json,
}

pub fn write_report(
    mut writer: impl Write,
    format: ExportFormat,
    report: &OverviewReport,
) -> Result<()> {
    match format {
        ExportFormat::Json => {
            serde_json::to_writer_pretty(&mut writer, report)
                .map_err(|error| AppError::Other(error.to_string()))?;
            writeln!(writer).map_err(|error| AppError::Other(error.to_string()))?;
        }
        ExportFormat::Csv => write_csv(writer, report)?,
    }
    Ok(())
}

fn write_csv(writer: impl Write, report: &OverviewReport) -> Result<()> {
    let mut csv = csv::Writer::from_writer(writer);
    csv.write_record([
        "section",
        "name",
        "date",
        "current",
        "previous",
        "active_users",
        "sessions",
        "engagement_rate",
    ])
    .map_err(|error| AppError::Other(error.to_string()))?;
    let summaries = [
        ("active_users", &report.summary.active_users),
        ("sessions", &report.summary.sessions),
        ("engagement_rate", &report.summary.engagement_rate),
        ("key_events", &report.summary.key_events),
    ];
    for (name, metric) in summaries {
        csv.write_record([
            "summary",
            name,
            "",
            &metric.current.to_string(),
            &metric.previous.map(|v| v.to_string()).unwrap_or_default(),
            "",
            "",
            "",
        ])
        .map_err(|error| AppError::Other(error.to_string()))?;
    }
    for point in &report.trend {
        csv.write_record([
            "trend",
            if point.comparison {
                "previous"
            } else {
                "current"
            },
            &point.date.to_string(),
            "",
            "",
            &point.active_users.to_string(),
            "",
            "",
        ])
        .map_err(|error| AppError::Other(error.to_string()))?;
    }
    for row in &report.channels {
        csv.write_record([
            "channel",
            &row.channel,
            "",
            "",
            "",
            &row.active_users.to_string(),
            &row.sessions.to_string(),
            &row.engagement_rate.to_string(),
        ])
        .map_err(|error| AppError::Other(error.to_string()))?;
    }
    csv.flush()
        .map_err(|error| AppError::Other(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ga4::*;
    use chrono::{NaiveDate, Utc};

    fn report() -> OverviewReport {
        let metric = MetricValue {
            current: 10.0,
            previous: Some(5.0),
        };
        OverviewReport {
            property_id: "1".into(),
            start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            summary: SummaryMetrics {
                active_users: metric.clone(),
                sessions: metric.clone(),
                engagement_rate: metric.clone(),
                key_events: metric,
            },
            trend: vec![],
            channels: vec![ChannelRow {
                channel: "Email, newsletter".into(),
                active_users: 2.0,
                sessions: 3.0,
                engagement_rate: 0.5,
            }],
            quota: QuotaSnapshot::default(),
            fetched_at: Utc::now(),
        }
    }

    #[test]
    fn csv_escapes_commas() {
        let mut output = Vec::new();
        write_report(&mut output, ExportFormat::Csv, &report()).unwrap();
        assert!(String::from_utf8(output)
            .unwrap()
            .contains("\"Email, newsletter\""));
    }
}
