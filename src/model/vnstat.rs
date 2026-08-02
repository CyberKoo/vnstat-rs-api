use serde::{Deserialize, Serialize};

/// Top-level vnstat data containing all monitored network interfaces and
/// version information about the vnstat daemon and the JSON output format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VnstatData {
    /// A list of network interfaces and their traffic statistics.
    pub interfaces: Vec<Interface>,
    /// The version of the JSON output format used by vnstat.
    pub jsonversion: String,
    /// The version of the vnstat daemon that produced this data.
    pub vnstatversion: String,
}

/// Traffic statistics and metadata for a single network interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interface {
    /// A human-readable alias or label for the interface.
    pub alias: String,
    /// The timestamp at which this interface was first created / observed.
    pub created: Created,
    /// The system-level name of the interface (e.g. `eth0`, `wlan0`).
    pub name: String,
    /// Aggregated traffic data across all time periods for this interface.
    pub traffic: Traffic,
    /// The timestamp of the most recent update for this interface.
    pub updated: Updated,
}

/// The creation timestamp of a network interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Created {
    /// The date on which the interface was created.
    pub date: Date,
    /// Unix epoch timestamp (seconds) of the creation time.
    pub timestamp: i64,
}

/// The last-updated timestamp of a network interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Updated {
    /// The date of the most recent update.
    pub date: Date,
    /// The time of day of the most recent update.
    pub time: Time,
    /// Unix epoch timestamp (seconds) of the most recent update.
    pub timestamp: i64,
}

/// A calendar date with optional day and month fields.
///
/// The `day` and `month` fields may be `None` when the date represents a
/// coarser granularity (e.g. a year-only date).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Date {
    /// Day of the month (1–31). `None` when not applicable.
    pub day: Option<u8>,
    /// Month of the year (1–12). `None` when not applicable.
    pub month: Option<u8>,
    /// Four-digit year (e.g. 2025).
    pub year: i32,
}

/// A time of day expressed in hours and minutes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Time {
    /// Hour of the day (0–23).
    pub hour: u8,
    /// Minute of the hour (0–59).
    pub minute: u8,
}

/// Aggregated traffic data for a single interface, organised by time period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Traffic {
    /// Per-day traffic records.
    pub day: Vec<DayRecord>,
    /// Per-5-minute traffic records (high-resolution).
    pub fiveminute: Vec<FiveMinuteRecord>,
    /// Per-hour traffic records.
    pub hour: Vec<HourRecord>,
    /// Per-month traffic records.
    pub month: Vec<MonthRecord>,
    /// Top traffic day records.
    pub top: Vec<TopRecord>,
    /// Cumulative total traffic since the interface was created.
    pub total: Total,
    /// Per-year traffic records.
    pub year: Vec<YearRecord>,
}

/// A single day's traffic record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayRecord {
    /// The date of this record.
    pub date: Date,
    /// A sequential identifier for this record.
    pub id: u32,
    /// Total bytes received on this day.
    pub rx: u64,
    /// Unix epoch timestamp (seconds) for this record.
    pub timestamp: i64,
    /// Total bytes transmitted on this day.
    pub tx: u64,
}

/// A single 5-minute interval traffic record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiveMinuteRecord {
    /// The date of this record.
    pub date: Date,
    /// A sequential identifier for this record.
    pub id: u32,
    /// Total bytes received in this 5-minute interval.
    pub rx: u64,
    /// The time at which this 5-minute interval started.
    pub time: Time,
    /// Unix epoch timestamp (seconds) for this record.
    pub timestamp: i64,
    /// Total bytes transmitted in this 5-minute interval.
    pub tx: u64,
}

/// A single hour's traffic record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourRecord {
    /// The date of this record.
    pub date: Date,
    /// A sequential identifier for this record.
    pub id: u32,
    /// Total bytes received in this hour.
    pub rx: u64,
    /// The starting time of this hour.
    pub time: Time,
    /// Unix epoch timestamp (seconds) for this record.
    pub timestamp: i64,
    /// Total bytes transmitted in this hour.
    pub tx: u64,
}

/// A single month's traffic record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthRecord {
    /// The month and year of this record.
    pub date: MonthDate,
    /// A sequential identifier for this record.
    pub id: u32,
    /// Total bytes received in this month.
    pub rx: u64,
    /// Unix epoch timestamp (seconds) for this record.
    pub timestamp: i64,
    /// Total bytes transmitted in this month.
    pub tx: u64,
}

/// A month-and-year date used in monthly traffic records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthDate {
    /// Month of the year (1–12).
    pub month: u8,
    /// Four-digit year (e.g. 2025).
    pub year: i32,
}

/// A "top" traffic day record, representing the single busiest day
/// (or one of the busiest days) for the interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopRecord {
    /// The date of this top record.
    pub date: Date,
    /// A sequential identifier for this record.
    pub id: u32,
    /// Total bytes received on this day.
    pub rx: u64,
    /// Unix epoch timestamp (seconds) for this record.
    pub timestamp: i64,
    /// Total bytes transmitted on this day.
    pub tx: u64,
}

/// Cumulative total traffic since the interface was created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Total {
    /// Total bytes received since creation.
    pub rx: u64,
    /// Total bytes transmitted since creation.
    pub tx: u64,
}

/// A single year's traffic record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YearRecord {
    /// The year of this record.
    pub date: YearDate,
    /// A sequential identifier for this record.
    pub id: u32,
    /// Total bytes received in this year.
    pub rx: u64,
    /// Unix epoch timestamp (seconds) for this record.
    pub timestamp: i64,
    /// Total bytes transmitted in this year.
    pub tx: u64,
}

/// A year-only date used in yearly traffic records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YearDate {
    /// Four-digit year (e.g. 2025).
    pub year: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_sample_vnstat_json() {
        let data: VnstatData =
            serde_json::from_str(crate::test_support::SAMPLE_VNSTAT_JSON).unwrap();
        assert_eq!(data.vnstatversion, "2.13");
        assert_eq!(data.interfaces.len(), 2);

        let eth0 = &data.interfaces[0];
        assert_eq!(eth0.name, "eth0");
        assert_eq!(eth0.alias, "");
        assert_eq!(eth0.traffic.day.len(), 2);
        assert_eq!(eth0.traffic.hour.len(), 1);
        assert_eq!(eth0.traffic.month.len(), 1);
        assert_eq!(eth0.traffic.year.len(), 1);
        assert_eq!(eth0.traffic.top.len(), 1);
        assert_eq!(eth0.traffic.fiveminute.len(), 1);
        assert_eq!(eth0.traffic.total.rx, 123456789);
        assert_eq!(eth0.traffic.total.tx, 987654321);
        assert_eq!(eth0.traffic.day[0].rx, 100000000);
        assert_eq!(eth0.traffic.day[0].date.year, 2026);
        assert_eq!(eth0.updated.timestamp, 1780331400);
        assert_eq!(eth0.created.timestamp, 1755223044);

        let wlan0 = &data.interfaces[1];
        assert_eq!(wlan0.alias, "Wireless");
        assert!(wlan0.traffic.day.is_empty());
        assert_eq!(wlan0.traffic.total.rx, 1);
    }

    #[test]
    fn serializes_back_to_valid_json() {
        let data: VnstatData =
            serde_json::from_str(crate::test_support::SAMPLE_VNSTAT_JSON).unwrap();
        let json = serde_json::to_string(&data).unwrap();
        let round: VnstatData = serde_json::from_str(&json).unwrap();
        assert_eq!(round.interfaces[0].name, "eth0");
        assert_eq!(round.interfaces[1].traffic.total.tx, 2);
    }

    #[test]
    fn date_with_optional_fields() {
        let date: Date = serde_json::from_str(r#"{"year":2026}"#).unwrap();
        assert_eq!(date.year, 2026);
        assert_eq!(date.month, None);
        assert_eq!(date.day, None);

        let date: Date = serde_json::from_str(r#"{"year":2026,"month":6,"day":2}"#).unwrap();
        assert_eq!(date.month, Some(6));
        assert_eq!(date.day, Some(2));
    }
}
