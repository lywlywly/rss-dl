use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SeriesConfig {
    pub update_time: NaiveTime,
    pub keyword: String,
    pub pattern: String,
    pub start_date: NaiveDate,
    pub latest_downloaded: u32,
    pub offset: Option<u32>,
    pub rename_pattern: Option<String>,
    pub target_directory: Option<String>,
}

#[derive(Debug)]
pub struct Task {
    pub name: String,
    pub scheduled_time: DateTime<Utc>,
    pub keyword: String,
    pub pattern: Regex,
    pub episode: u32,
    pub offset: Option<u32>,
    pub rename_pattern: Option<String>,
    pub target_directory: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub feed_url: String,
}
