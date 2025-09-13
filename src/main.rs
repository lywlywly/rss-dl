mod globals;
mod types;
mod utils;

use utils::{build_tasks, process_tasks};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tasks = build_tasks().await?;
    process_tasks(tasks, true, 3).await?;
    Ok(())
}
