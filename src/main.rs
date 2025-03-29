use anyhow::Result;
use indicatif::MultiProgress;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::Mutex;

mod api;
mod speed_test;

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
        .map(|x| Arc::new(Mutex::new(speed_test::DownloadType::Url(x))))
        .collect::<Vec<_>>();

    urls.push(Arc::new(Mutex::new(speed_test::DownloadType::Total)));

    let mut speed_test = speed_test::SpeedTest::new(&mut urls[..]).await?;

    let mp = MultiProgress::new();
    let data = speed_test.start(&mp, &mut urls[..]).await?;

    let time_taken_secs = data.total_time_secs;

    println!(
        "  Download: {:.2} MB/s\n            {:.2} Mbps\n            {} byte/s in {:.2}s.",
        data.avg_speed,
        data.avg_speed * 8.0,
        data.total_size,
        time_taken_secs
    );
    Ok(())
}
