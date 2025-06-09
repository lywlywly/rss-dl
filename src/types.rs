use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use regex::Regex;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SeriesConfig {
    pub update_time: NaiveTime,
    pub keyword: String,
    #[serde(deserialize_with = "single_or_vec")]
    pub pattern: Vec<String>,
    pub start_date: NaiveDate,
    pub latest_downloaded: u32,
    pub offset: Option<u32>,
    pub rename_pattern: Option<String>,
    pub target_directory: Option<String>,
    pub skip: Option<u32>,
    pub feed_url: Option<String>,
}

fn single_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SingleOrVec {
        Single(String),
        Vec(Vec<String>),
    }

    match SingleOrVec::deserialize(deserializer)? {
        SingleOrVec::Single(s) => Ok(vec![s]),
        SingleOrVec::Vec(v) => Ok(v),
    }
}

#[derive(Debug)]
pub struct Task {
    pub name: String,
    pub scheduled_time: DateTime<Utc>,
    pub keyword: String,
    pub pattern: Vec<Regex>,
    pub episode: u32,
    pub offset: Option<u32>,
    pub rename_pattern: Option<String>,
    pub target_directory: Option<String>,
    pub feed_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub feed_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_yaml;

    #[derive(Debug, Deserialize)]
    struct TestStruct {
        #[serde(deserialize_with = "single_or_vec")]
        pattern: Vec<String>,
    }

    #[test]
    fn test_single_pattern() {
        let yaml = r#"
pattern: "only-one-pattern"
"#;
        let parsed: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.pattern, vec!["only-one-pattern"]);
    }

    #[test]
    fn test_single_pattern_seq() {
        let yaml = r#"
pattern: 
  - "only-one-pattern"
"#;
        let parsed: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.pattern, vec!["only-one-pattern"]);
    }

    #[test]
    fn test_multiple_patterns() {
        let yaml = r#"
pattern:
  - "first-pattern"
  - "second-pattern"
"#;
        let parsed: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.pattern, vec!["first-pattern", "second-pattern"]);
    }

    #[test]
    fn test_empty_list() {
        let yaml = r#"
pattern: []
"#;
        let parsed: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.pattern, Vec::<String>::new());
    }

    #[test]
    fn test_invalid_type() {
        let yaml = r#"
pattern: 12345
"#;
        let result: Result<TestStruct, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err()); // Parsing an integer into a string should fail
    }
}
