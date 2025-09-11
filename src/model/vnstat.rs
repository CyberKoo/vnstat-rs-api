use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VnstatData {
    pub interfaces: Vec<Interface>,
    pub jsonversion: String,
    pub vnstatversion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interface {
    pub alias: String,
    pub created: Created,
    pub name: String,
    pub traffic: Traffic,
    pub updated: Updated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Created {
    pub date: Date,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Updated {
    pub date: Date,
    pub time: Time,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Date {
    pub day: Option<u8>,
    pub month: Option<u8>,
    pub year: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Traffic {
    pub day: Vec<DayRecord>,
    pub fiveminute: Vec<FiveMinuteRecord>,
    pub hour: Vec<HourRecord>,
    pub month: Vec<MonthRecord>,
    pub top: Vec<TopRecord>,
    pub total: Total,
    pub year: Vec<YearRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayRecord {
    pub date: Date,
    pub id: u32,
    pub rx: u64,
    pub timestamp: i64,
    pub tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiveMinuteRecord {
    pub date: Date,
    pub id: u32,
    pub rx: u64,
    pub time: Time,
    pub timestamp: i64,
    pub tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourRecord {
    pub date: Date,
    pub id: u32,
    pub rx: u64,
    pub time: Time,
    pub timestamp: i64,
    pub tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthRecord {
    pub date: MonthDate,
    pub id: u32,
    pub rx: u64,
    pub timestamp: i64,
    pub tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthDate {
    pub month: u8,
    pub year: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopRecord {
    pub date: Date,
    pub id: u32,
    pub rx: u64,
    pub timestamp: i64,
    pub tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Total {
    pub rx: u64,
    pub tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YearRecord {
    pub date: YearDate,
    pub id: u32,
    pub rx: u64,
    pub timestamp: i64,
    pub tx: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YearDate {
    pub year: i32,
}
