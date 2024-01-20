use anyhow::{anyhow, Result};
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Debug, Serialize, Deserialize)]
struct Location {
    city: String,
    country: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Target {
    name: String,
    url: String,
    location: Location,
}

#[derive(Debug, Serialize, Deserialize)]
struct Client {
    ip: String,
    asn: String,
    isp: String,
    location: Location,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse {
    client: Client,
    targets: Vec<Target>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = reqwest::Client::new();
    let url = "https://api.fast.com/netflix/speedtest/v2";
    let params = [
        ("https", "false"),
        ("token", "YXNkZmFzZGxmbnNkYWZoYXNkZmhrYWxm"),
    ];

    print!("Finding hosts... ");
    std::io::stdout().flush().unwrap();

    let response = client.get(url).query(&params).send().await?;
    let response = response.text().await?;
    let response = serde_json::from_str::<ApiResponse>(&response)?;
    println!("{} hosts found.", response.targets.len());

    let target = response
        .targets
        .first()
        .ok_or(anyhow!("Unable to find first target"))?;
    let url = target.url.clone();

    let download = reqwest::get(url);

    let start_time = std::time::Instant::now();

    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    tokio::spawn(async move {
        let Ok(mut download) = download.await else {
            eprintln!("Failed to get chunk");
            return Err(anyhow!("failed to get chunk"));
        };

        let total_size = download.content_length().unwrap_or(25_000_000);
        tx.send(total_size as usize).await?;

        while let Some(chunk) = download.chunk().await? {
            tx.send(chunk.len()).await?;
        }
        Ok(())
    });

    let pb = match rx.recv().await {
        Some(size) => ProgressBar::new(size as u64),
        None => return Err(anyhow!("Failed to get size")),
    };

    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .unwrap());

    let mut current_size = 0;
    while let Some(progress) = rx.recv().await {
        //pb.set_position(progress as u64);
        pb.inc(progress as u64);
        current_size += progress;
    }

    pb.finish_and_clear();

    let current_size_mib = current_size as f32 / 1_000_000.0;
    let time_taken_secs = std::time::Instant::now()
        .duration_since(start_time)
        .as_secs_f32();

    let speed_mib_s = current_size_mib / time_taken_secs;
    println!(
        "  Download: {:.2} MB/s | {:.2} Mbps | {} byte/s in {:.2}s.",
        speed_mib_s,
        speed_mib_s * 8.0,
        current_size,
        time_taken_secs
    );
    Ok(())
}
