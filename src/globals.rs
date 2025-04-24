use crate::types::{Config, SeriesConfig};
use once_cell::sync::Lazy;
use std::{collections::HashMap, fs, sync::Mutex};

pub static GLOBAL_MAP: Lazy<Mutex<HashMap<String, SeriesConfig>>> = Lazy::new(|| {
    let contents = fs::read_to_string("tasks.yaml").expect("Failed to read tasks.yaml");
    let map: HashMap<String, SeriesConfig> = serde_yaml::from_str(&contents).unwrap();
    Mutex::new(map)
});

pub static GLOBAL_CONFIG: Lazy<Mutex<Config>> = Lazy::new(|| {
    let contents = fs::read_to_string("config.yaml").expect("Failed to read config.yaml");
    let config: Config = serde_yaml::from_str(&contents).unwrap();
    Mutex::new(config)
});
