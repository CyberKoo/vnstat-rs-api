use crate::model::vnstat as vn;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(super) struct Interface {
    pub alias: String,
    pub created: Created,
    pub name: String,
    pub traffic: Traffic,
    pub updated: Updated,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct Created {
    pub date: Date,
    pub timestamp: i64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct Updated {
    pub date: Date,
    pub time: Time,
    pub timestamp: i64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct Date {
    pub day: Option<u8>,
    pub month: Option<u8>,
    pub year: i32,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct Time {
    pub hour: u8,
    pub minute: u8,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct Traffic {
    pub day: Vec<DayRecord>,
    pub fiveminute: Vec<FiveMinuteRecord>,
    pub hour: Vec<HourRecord>,
    pub month: Vec<MonthRecord>,
    pub top: Vec<TopRecord>,
    pub total: Total,
    pub year: Vec<YearRecord>,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct DayRecord {
    pub date: Date,
    pub id: u32,
    pub rx: u64,
    pub timestamp: i64,
    pub tx: u64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct FiveMinuteRecord {
    pub date: Date,
    pub id: u32,
    pub rx: u64,
    pub time: Time,
    pub timestamp: i64,
    pub tx: u64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct HourRecord {
    pub date: Date,
    pub id: u32,
    pub rx: u64,
    pub time: Time,
    pub timestamp: i64,
    pub tx: u64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct MonthRecord {
    pub date: MonthDate,
    pub id: u32,
    pub rx: u64,
    pub timestamp: i64,
    pub tx: u64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct MonthDate {
    pub month: u8,
    pub year: i32,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct TopRecord {
    pub date: Date,
    pub id: u32,
    pub rx: u64,
    pub timestamp: i64,
    pub tx: u64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct Total {
    pub rx: u64,
    pub tx: u64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct YearRecord {
    pub date: YearDate,
    pub id: u32,
    pub rx: u64,
    pub timestamp: i64,
    pub tx: u64,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct YearDate {
    pub year: i32,
}

impl From<vn::Interface> for Interface {
    fn from(v: vn::Interface) -> Self {
        Self {
            alias: v.alias,
            created: v.created.into(),
            name: v.name,
            traffic: v.traffic.into(),
            updated: v.updated.into(),
        }
    }
}
impl From<vn::Created> for Created {
    fn from(v: vn::Created) -> Self {
        Self {
            date: v.date.into(),
            timestamp: v.timestamp,
        }
    }
}
impl From<vn::Updated> for Updated {
    fn from(v: vn::Updated) -> Self {
        Self {
            date: v.date.into(),
            time: v.time.into(),
            timestamp: v.timestamp,
        }
    }
}
impl From<vn::Date> for Date {
    fn from(v: vn::Date) -> Self {
        Self {
            day: v.day,
            month: v.month,
            year: v.year,
        }
    }
}
impl From<vn::Time> for Time {
    fn from(v: vn::Time) -> Self {
        Self {
            hour: v.hour,
            minute: v.minute,
        }
    }
}
impl From<vn::Traffic> for Traffic {
    fn from(v: vn::Traffic) -> Self {
        Self {
            day: v.day.into_iter().map(Into::into).collect(),
            fiveminute: v.fiveminute.into_iter().map(Into::into).collect(),
            hour: v.hour.into_iter().map(Into::into).collect(),
            month: v.month.into_iter().map(Into::into).collect(),
            top: v.top.into_iter().map(Into::into).collect(),
            total: v.total.into(),
            year: v.year.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<vn::DayRecord> for DayRecord {
    fn from(v: vn::DayRecord) -> Self {
        Self {
            date: v.date.into(),
            id: v.id,
            rx: v.rx,
            timestamp: v.timestamp,
            tx: v.tx,
        }
    }
}
impl From<vn::FiveMinuteRecord> for FiveMinuteRecord {
    fn from(v: vn::FiveMinuteRecord) -> Self {
        Self {
            date: v.date.into(),
            id: v.id,
            rx: v.rx,
            time: v.time.into(),
            timestamp: v.timestamp,
            tx: v.tx,
        }
    }
}
impl From<vn::HourRecord> for HourRecord {
    fn from(v: vn::HourRecord) -> Self {
        Self {
            date: v.date.into(),
            id: v.id,
            rx: v.rx,
            time: v.time.into(),
            timestamp: v.timestamp,
            tx: v.tx,
        }
    }
}
impl From<vn::MonthRecord> for MonthRecord {
    fn from(v: vn::MonthRecord) -> Self {
        Self {
            date: v.date.into(),
            id: v.id,
            rx: v.rx,
            timestamp: v.timestamp,
            tx: v.tx,
        }
    }
}
impl From<vn::MonthDate> for MonthDate {
    fn from(v: vn::MonthDate) -> Self {
        Self {
            month: v.month,
            year: v.year,
        }
    }
}
impl From<vn::TopRecord> for TopRecord {
    fn from(v: vn::TopRecord) -> Self {
        Self {
            date: v.date.into(),
            id: v.id,
            rx: v.rx,
            timestamp: v.timestamp,
            tx: v.tx,
        }
    }
}
impl From<vn::Total> for Total {
    fn from(v: vn::Total) -> Self {
        Self { rx: v.rx, tx: v.tx }
    }
}
impl From<vn::YearRecord> for YearRecord {
    fn from(v: vn::YearRecord) -> Self {
        Self {
            date: v.date.into(),
            id: v.id,
            rx: v.rx,
            timestamp: v.timestamp,
            tx: v.tx,
        }
    }
}
impl From<vn::YearDate> for YearDate {
    fn from(v: vn::YearDate) -> Self {
        Self { year: v.year }
    }
}

/// Compact summary of a single interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InterfaceSummary {
    name: String,
    alias: String,
    total: Total,
    today_rx: u64,
    today_tx: u64,
    updated_timestamp: i64,
}
impl From<&vn::Interface> for InterfaceSummary {
    fn from(iface: &vn::Interface) -> Self {
        let today = iface.traffic.day.last();
        Self {
            name: iface.name.clone(),
            alias: iface.alias.clone(),
            total: iface.traffic.total.clone().into(),
            today_rx: today.map(|r| r.rx).unwrap_or(0),
            today_tx: today.map(|r| r.tx).unwrap_or(0),
            updated_timestamp: iface.updated.timestamp,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AggregateStats {
    total_interfaces: usize,
    total_rx: u64,
    total_tx: u64,
}

impl From<&[vn::Interface]> for AggregateStats {
    fn from(interfaces: &[vn::Interface]) -> Self {
        Self {
            total_interfaces: interfaces.len(),
            total_rx: interfaces.iter().map(|i| i.traffic.total.rx).sum(),
            total_tx: interfaces.iter().map(|i| i.traffic.total.tx).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_dto_preserves_existing_json_contract() {
        let data: vn::VnstatData =
            serde_json::from_str(crate::test_support::SAMPLE_VNSTAT_JSON).unwrap();
        for interface in data.interfaces {
            let original = serde_json::to_value(&interface).unwrap();
            let api_response = serde_json::to_value(Interface::from(interface)).unwrap();
            assert_eq!(api_response, original);
        }
    }
}
