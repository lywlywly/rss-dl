mod globals;
mod types;
mod utils;

use utils::{build_tasks, process_tasks};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tasks = build_tasks().unwrap();
    process_tasks(tasks, true, 2).await?;
    Ok(())
}
