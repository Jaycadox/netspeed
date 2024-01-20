use anyhow::{anyhow, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

mod api;

enum DownloadType<'a> {
    Url(&'a mut api::Target),
    Total,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = api::Api::new();
    print!("Finding hosts... ");
    std::io::stdout().flush().unwrap();

    let mut response = client.get_hosts().await?;
    println!("{} hosts found.", response.targets.len());

    let mut urls = response
        .targets
        .iter_mut()
        .map(|x| Arc::new(Mutex::new(DownloadType::Url(x))))
        .collect::<Vec<_>>();
    let urls_count = urls.len();
    urls.push(Arc::new(Mutex::new(DownloadType::Total)));

    let start_time = std::time::Instant::now();

    let mp = MultiProgress::new();

    let current_size = AtomicU64::new(0);
    let total_size = AtomicU64::new(0);

    let downloads = urls
        .iter()
        .map(|url| async {
            let url = &mut *url.lock().await;
            match url {
                DownloadType::Total => {
                    let pb = mp.add(ProgressBar::new(total_size.load(Ordering::Relaxed)));
                pb.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.green} [{bar:.green}] [{msg}] [{elapsed_precise}] {bytes}/{total_bytes}",
                    )
                    .unwrap(),
                );
                while current_size.load(Ordering::Relaxed) == 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
                while current_size.load(Ordering::Relaxed) < total_size.load(Ordering::Relaxed) {
                    let total_size = total_size.load(Ordering::Relaxed);
                    let current_size = current_size.load(Ordering::Relaxed);
                    pb.set_length(total_size);
                    pb.set_position(current_size);
                    let elapsed = std::time::Instant::now()
                        .duration_since(start_time)
                        .as_secs_f32();

                    let downloaded_mb = current_size as f32 / 1_000_000.0;
                    let speed = downloaded_mb / elapsed;
                    pb.set_message(format!("{speed:.2} Mb/s"));
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
                pb.finish_and_clear();
                },
                DownloadType::Url(url) => {
                    let pb = mp.add(ProgressBar::new(0));
                    let content_length = url.content_length().await?;
            let content_length = content_length
                / urls_count as u64;
            total_size.fetch_add(content_length, Ordering::Relaxed);

            pb.set_length(content_length);
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{bar}]",
                )
                .unwrap(),
            );

            while let Ok(Some(chunk)) = url.response().ok_or(anyhow!("Unable to get response"))?.chunk().await {
                //pb.set_position(progress as u64);
                pb.inc(chunk.len() as u64);
                current_size.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                if pb.position() >= content_length {
                    break;
                }
            }

            pb.finish_and_clear();
                },
            }

            Ok::<(), anyhow::Error>(())
        })
        .collect::<Vec<_>>();

    futures::future::try_join_all(downloads).await?;

    let current_size = current_size.load(Ordering::Relaxed);

    let current_size_mib = current_size as f32 / 1_000_000.0;
    let time_taken_secs = std::time::Instant::now()
        .duration_since(start_time)
        .as_secs_f32();

    let speed_mib_s = current_size_mib / time_taken_secs;
    println!(
        "  Download: {:.2} MB/s\n            {:.2} Mbps\n            {} byte/s in {:.2}s.",
        speed_mib_s,
        speed_mib_s * 8.0,
        current_size,
        time_taken_secs
    );
    Ok(())
}
