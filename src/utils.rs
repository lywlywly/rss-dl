use crate::globals::{GLOBAL_CONFIG, GLOBAL_MAP};
use crate::types::{SeriesConfig, Task};
use chrono::{DateTime, Utc};
use futures::stream::{FuturesUnordered, StreamExt};
use quick_xml::de::from_reader;
use regex::Regex;
use reqwest::get;
use serde::Deserialize;
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::error::Error;
use std::path::Path;
use std::process::Command;
use std::sync::MutexGuard;
use std::{fmt, fs};
use strfmt::strfmt;
use tokio::time::{Duration, sleep};

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.scheduled_time == other.scheduled_time
    }
}

impl Eq for Task {}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        self.scheduled_time.cmp(&other.scheduled_time)
    }
}

#[derive(Debug)]
pub enum QueueType<T: Ord> {
    Queue(VecDeque<T>),
    PriorityQueue(BinaryHeap<Reverse<T>>),
}

#[allow(dead_code)]
impl<T: Ord> QueueType<T> {
    pub fn from_iter<I: IntoIterator<Item = T>>(iter: I, use_priority_queue: bool) -> Self {
        if use_priority_queue {
            let heap = iter.into_iter().map(Reverse).collect();
            QueueType::PriorityQueue(heap)
        } else {
            let mut vec: Vec<T> = iter.into_iter().collect();
            vec.sort(); // ascending
            QueueType::Queue(VecDeque::from(vec))
        }
    }

    pub fn enqueue(&mut self, item: T) {
        match self {
            QueueType::Queue(q) => q.push_back(item),
            QueueType::PriorityQueue(pq) => pq.push(Reverse(item)),
        }
    }

    pub fn dequeue(&mut self) -> Option<T> {
        match self {
            QueueType::Queue(q) => q.pop_front(),
            QueueType::PriorityQueue(pq) => pq.pop().map(|r| r.0),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            QueueType::Queue(q) => q.is_empty(),
            QueueType::PriorityQueue(pq) => pq.is_empty(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct TextNode(#[serde(rename = "$value")] pub String);

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Guid {
    #[serde(rename = "$value")]
    pub value: String,
    #[serde(rename = "@isPermaLink")]
    pub is_perma_link: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Enclosure {
    #[serde(rename = "@url")]
    pub url: String,
    #[serde(rename = "@length")]
    pub length: String,
    #[serde(rename = "@type")]
    pub mime_type: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Category {
    #[serde(rename = "$value")]
    pub value: String,
    #[serde(rename = "@domain")]
    pub domain: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Item {
    pub title: TextNode,
    pub link: TextNode,
    pub description: TextNode,
    pub guid: Guid,
    pub author: TextNode,
    pub enclosure: Enclosure,
    #[serde(rename = "pubDate")]
    pub pub_date: TextNode,
    pub category: Category,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Channel {
    pub title: TextNode,
    pub link: TextNode,
    pub description: TextNode,
    #[serde(rename = "lastBuildDate")]
    pub last_build_date: TextNode,
    pub language: TextNode,
    pub generator: TextNode,
    pub copyright: TextNode,
    pub item: Vec<Item>,
}

#[derive(Debug, Deserialize)]
pub struct Rss {
    pub channel: Channel,
}

#[derive(Debug, PartialEq, Eq)]
pub enum State {
    Downloading,
    Uploading,
    StalledDownloading,
    StalledUploading,
    StoppedDownloading,
    StoppedUploading,
    DownloadingMetadata,
    MissingFiles,
}

impl State {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "downloading" => Some(State::Downloading),
            "uploading" => Some(State::Uploading),
            "stalledDL" => Some(State::StalledDownloading),
            "stalledUP" => Some(State::StalledUploading),
            "stoppedDL" => Some(State::StoppedDownloading),
            "stoppedUP" => Some(State::StoppedUploading),
            "metaDL" => Some(State::DownloadingMetadata),
            "missingFiles" => Some(State::MissingFiles),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct RenameError(&'static str);
impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rename error: {}", self.0)
    }
}

impl Error for RenameError {}

pub fn rename_video(
    download_path: &str,
    file_name: &str,
    rename_pattern: &str,
    episode: u32,
    target_directory: &str,
) -> Result<(), Box<dyn Error>> {
    let file_path = Path::new(download_path).join(file_name);
    if !file_path.is_file() {
        return Err(Box::new(RenameError("File does not exist")));
    }

    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| RenameError("Missing file extension"))?;
    let ep_str = format!("{:02}", episode);

    let mut vars = HashMap::new();
    vars.insert("ep".to_string(), ep_str);
    let new_name = format!("{}.{}", strfmt(rename_pattern, &vars)?, ext);
    let new_path = Path::new(target_directory).join(new_name);

    println!(
        "Copying from {} to {}",
        file_path.display(),
        new_path.display()
    );
    fs::copy(&file_path, &new_path)?;
    Ok(())
}

#[derive(Debug)]
pub enum MatchError {
    NoMatch(u32),
    MultipleMatches(u32),
}

impl fmt::Display for MatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MatchError::NoMatch(ep) => write!(f, "No item matched episode {}", ep),
            MatchError::MultipleMatches(ep) => write!(f, "Multiple items matched episode {}", ep),
        }
    }
}

impl std::error::Error for MatchError {}

fn match_item_by_title(mut rss: Rss, pattern: &Regex, ep: u32) -> Result<Item, MatchError> {
    let mut matches: Vec<Item> = rss
        .channel
        .item
        .drain(..)
        .filter(|item| {
            if let Some(caps) = pattern.captures(&item.title.0) {
                if let Some(episode_str) = caps.get(1) {
                    return episode_str.as_str().parse::<u32>().ok() == Some(ep);
                }
            }
            false
        })
        .collect();

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(MatchError::NoMatch(ep)), // TODO
        _ => Err(MatchError::MultipleMatches(ep)),
    }
}

pub async fn wait_until(target: DateTime<Utc>) {
    let now = Utc::now();
    if let Some(wait_duration) = (target - now).to_std().ok() {
        println!("Waiting for {} seconds...", wait_duration.as_secs());
        sleep(wait_duration).await;
    } else {
        println!("Target time is in the past. Skipping wait.");
    }
}

pub fn write_to_config() -> Result<(), Box<dyn std::error::Error>> {
    let guard: MutexGuard<'_, HashMap<String, SeriesConfig>> = GLOBAL_MAP.lock().unwrap();
    let yaml = serde_yaml::to_string(&*guard)?;
    fs::write("tasks.yaml", yaml)?;
    Ok(())
}

pub async fn fetch_rss_by_keyword(keyword: &str) -> Result<Rss, Box<dyn Error>> {
    let mut vars = HashMap::new();
    vars.insert(
        "keyword".to_string(),
        urlencoding::encode(keyword).into_owned(),
    );
    let full_url = strfmt(&GLOBAL_CONFIG.lock().unwrap().feed_url, &vars)?;
    let response = get(&full_url).await?;
    let bytes = response.bytes().await?;
    let rss: Rss = from_reader(bytes.as_ref())?;
    Ok(rss)
}

pub async fn fetch_and_match_rss_blocking(
    keyword: &str,
    pattern: &Regex,
    expected_episode: u32,
) -> Result<Item, Box<dyn Error>> {
    loop {
        println!(
            "Fetching RSS for keyword: {}, episode: {}",
            keyword, expected_episode
        );

        let rss = fetch_rss_by_keyword(keyword).await.unwrap();

        match match_item_by_title(rss, pattern, expected_episode) {
            Ok(item) => return Ok(item),
            Err(MatchError::NoMatch(_)) => {
                println!("No match found. Retrying in 5 minutes...");
                sleep(Duration::from_secs(5 * 60)).await;
            }
            Err(e) => return Err(Box::new(e)),
        }
    }
}

pub fn check_status_output(output: &str) -> Result<(State, String, String), Box<dyn Error>> {
    let pattern = Regex::new(r"\[\*\] (.*)[\s\S]*?State: (.*)[\s\S]*?Save path: (.*)")?;

    let caps = pattern
        .captures(output)
        .ok_or("Error extracting state from output")?;

    let filename = caps
        .get(1)
        .ok_or("Missing filename")?
        .as_str()
        .trim()
        .to_string();
    let state_str = caps.get(2).ok_or("Missing state")?.as_str().trim();
    let path = caps
        .get(3)
        .ok_or("Missing path")?
        .as_str()
        .trim()
        .to_string();

    let state = State::from_str(state_str).ok_or("Invalid state")?;

    Ok((state, path, filename))
}

pub fn check_status(hash: &str) -> Result<(State, String, String), Box<dyn Error>> {
    let output = Command::new("qbt")
        .args(["torrent", "list", "--hashes", hash])
        .output()?;

    if !output.status.success() {
        return Err(format!("Command failed with status {}", output.status).into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    check_status_output(&stdout)
}

pub async fn download_async(magnet: &str) -> Result<(String, String), Box<dyn Error>> {
    let hash = extract_hash_from_magnet_link(magnet)?;

    download(magnet)?;
    sleep(Duration::from_secs(10)).await;

    loop {
        let (status, download_path, file_name) = check_status(&hash)?; // or async
        println!("{:?}", status);

        match status {
            State::StalledUploading | State::Uploading | State::StoppedUploading => {
                println!("Downloading finished");
                pause(&hash)?;
                return Ok((download_path, file_name));
            }
            _ => {
                println!("Still downloading...");
                sleep(Duration::from_secs(30)).await;
            }
        }
    }
}

pub fn extract_hash_from_magnet_link(magnet: &str) -> Result<&str, &'static str> {
    magnet
        .split("btih:")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .ok_or("Error extracting hash")
}

pub fn download(magnet_link: &str) -> Result<(), Box<dyn Error>> {
    let output = Command::new("qbt")
        .args(["torrent", "add", magnet_link])
        .output()?;

    if output.status.success() {
        // println!("{}", String::from_utf8_lossy(&output.stderr)); FIXME: the library outputs to stderr
        Ok(())
    } else {
        Err(format!(
            "qbt exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

pub fn pause(hash: &str) -> Result<(), Box<dyn Error>> {
    let output = Command::new("qbt")
        .args(["torrent", "pause", "--hashes", hash])
        .output()?;

    if output.status.success() {
        // println!("{}", String::from_utf8_lossy(&output.stderr)); FIXME: the library writes to stderr
        Ok(())
    } else {
        Err(format!(
            "qbt pause failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

pub fn _check_qbt_availability() -> bool {
    Command::new("qbt")
        .args(["app", "version"])
        .status()
        .map_or(false, |status| status.success())
}

pub fn update_task(task: &Task) {
    if let Some(config) = GLOBAL_MAP.lock().unwrap().get_mut(&task.name) {
        config.latest_downloaded = task.episode;
    }
}

pub async fn process_task(current_task: &Task) -> Result<(), Box<dyn Error>> {
    println!(
        "Waiting for {} at {}",
        current_task.name,
        current_task
            .scheduled_time
            .with_timezone(&chrono::Local)
            .to_rfc2822()
    );
    wait_until(current_task.scheduled_time).await;
    println!(
        "[{}] Running: {}",
        chrono::Local::now().to_rfc2822(),
        current_task.name
    );
    let episode = current_task.episode + current_task.offset.unwrap_or(0);
    let result =
        fetch_and_match_rss_blocking(&current_task.keyword, &current_task.pattern, episode)
            .await
            .unwrap();
    let magnet_link = &result.enclosure.url;
    let (download_path, file_name) = download_async(magnet_link).await.unwrap();
    if let (Some(rename_pattern), Some(target_directory)) =
        (&current_task.rename_pattern, &current_task.target_directory)
    {
        rename_video(
            &download_path,
            &file_name,
            rename_pattern,
            episode,
            target_directory,
        )?;
    }
    update_task(&current_task);
    println!(
        "Adding task {} Episode {} Scheduled at {}",
        current_task.name,
        current_task.episode,
        current_task
            .scheduled_time
            .with_timezone(&chrono::Local)
            .to_rfc2822()
    );
    Ok(write_to_config()?)
}

pub fn build_tasks() -> Result<Vec<Task>, Box<dyn std::error::Error>> {
    GLOBAL_MAP
        .lock()
        .unwrap()
        .iter()
        .map(|(name, config)| {
            let naive_dt = config.start_date.and_time(config.update_time);
            let scheduled_time = DateTime::from_naive_utc_and_offset(
                naive_dt + chrono::Duration::days((config.latest_downloaded * 7) as i64),
                Utc,
            );

            let pattern = Regex::new(&config.pattern)?;

            Ok(Task {
                name: name.to_string(),
                scheduled_time,
                keyword: config.keyword.to_string(),
                pattern: pattern,
                episode: config.latest_downloaded + 1,
                offset: config.offset,
                rename_pattern: config.rename_pattern.clone(),
                target_directory: config.target_directory.clone(),
            })
        })
        .collect()
}

pub async fn process_tasks<'a>(
    tasks: impl IntoIterator<Item = &'a Task>,
    use_priority_queue: bool,
    concurrency_limit: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut queue = QueueType::from_iter(tasks, use_priority_queue);

    let mut in_progress = FuturesUnordered::new();

    while !queue.is_empty() || !in_progress.is_empty() {
        while in_progress.len() < concurrency_limit && !queue.is_empty() {
            let task_ref = queue.dequeue().unwrap();
            in_progress.push(process_task(task_ref));
        }
        in_progress.next().await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_1() {
        assert_eq!(_check_qbt_availability(), true);
    }

    #[test]
    fn test_valid_magnet() {
        let magnet = "magnet:?xt=urn:btih:ABC123XYZ4567890EXAMPLEHASH&dn=SomeFile";
        let hash = extract_hash_from_magnet_link(magnet).unwrap();
        assert_eq!(hash, "ABC123XYZ4567890EXAMPLEHASH");
    }

    #[test]
    fn test_valid_magnet_no_extra_params() {
        let magnet = "magnet:?xt=urn:btih:DEADBEEF1234567890";
        let hash = extract_hash_from_magnet_link(magnet).unwrap();
        assert_eq!(hash, "DEADBEEF1234567890");
    }

    #[test]
    fn test_missing_btih_prefix() {
        let magnet = "magnet:?xt=urn:sha1:XYZ123&dn=SomeFile";
        let result = extract_hash_from_magnet_link(magnet);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_string() {
        let magnet = "";
        let result = extract_hash_from_magnet_link(magnet);
        assert!(result.is_err());
    }
}
