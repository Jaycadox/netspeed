use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use anyhow::anyhow;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::sync::Mutex;
use crate::api;

pub enum DownloadType<'a> {
    Url(&'a mut api::Target),
    Total,
}

pub struct SpeedTest {
    total_size: u64,
    current_size: AtomicU64,
    start_time: std::time::Instant,
}

pub struct SpeedTestOutput {
    pub total_size: u64,
    pub total_time_secs: f32,
    pub avg_speed: f32,
}

impl SpeedTest {
    pub async fn new(targets: &[Arc<Mutex<DownloadType<'_>>>]) -> anyhow::Result<Self> {
        let total_size = Arc::new(AtomicU64::new(0));
        let target_count = targets.len();

        let resolve_sizes = targets
            .iter()
            .map(|x| {
                let x = Arc::clone(x);
                let total_size = Arc::clone(&total_size);
                async move {
                    let mut x = x.lock().await;
                    if let DownloadType::Url(target) = &mut *x {
                        total_size.fetch_add(target.content_length().await? / target_count as u64, Ordering::Relaxed);
                    }
                    Ok::<(), anyhow::Error>(())
                }
            }).collect::<Vec<_>>();
        futures::future::try_join_all(resolve_sizes).await?;

        Ok(Self {
            total_size: total_size.load(Ordering::Relaxed),
            current_size: AtomicU64::new(0),
            start_time: std::time::Instant::now(),
        })
    }

    pub async fn start(&mut self, mp: &MultiProgress, targets: &[Arc<Mutex<DownloadType<'_>>>]) -> anyhow::Result<SpeedTestOutput> {
        let target_count = targets.len();
        self.start_time = std::time::Instant::now();

        let (tx, mut rx) = tokio::sync::mpsc::channel(1024);

        let downloads = targets
            .iter()
            .map(|url| async {
                let url = &mut *url.lock().await;
                match url {
                    DownloadType::Total => self.total_progress_bar(&mp, tx.clone()).await?,
                    DownloadType::Url(url) => self.download_progress_bar(url, target_count, &mp).await?,
                }

                Ok::<(), anyhow::Error>(())
            })
            .collect::<Vec<_>>();

        futures::future::try_join_all(downloads).await?;
        mp.clear()?;

        let mut speeds = vec![];
        while let Ok(speed) = rx.try_recv() {
            speeds.push(speed);
        }

        let avg_speed = speeds.iter().sum::<f32>() / speeds.len() as f32;

        Ok(SpeedTestOutput {
            total_size: self.current_size.load(Ordering::Relaxed),
            total_time_secs: std::time::Instant::now().duration_since(self.start_time).as_secs_f32(),
            avg_speed,
        })
    }

    async fn download_progress_bar(&self, target: &mut api::Target, target_count: usize, mp: &MultiProgress) -> anyhow::Result<()> {
        let pb = mp.add(ProgressBar::new(0));
        let content_length = target.content_length().await?;
        let content_length = content_length
            / target_count as u64;

        pb.set_length(content_length);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{bar}]",
            ).unwrap(),
        );

        while let Ok(Some(chunk)) = target.response().ok_or(anyhow!("Unable to get response"))?.chunk().await {
            pb.inc(chunk.len() as u64);
            self.current_size.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            if pb.position() >= content_length {
                break;
            }
        }

        pb.finish();

        Ok(())
    }

    async fn total_progress_bar(&self, mp: &MultiProgress, speeds: tokio::sync::mpsc::Sender<f32>) -> anyhow::Result<()> {
        let pb = mp.add(ProgressBar::new(self.total_size));
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{bar:.green}] [{msg}] [{elapsed_precise}] {bytes}/{total_bytes}",
            )
                .unwrap(),
        );

        while self.current_size.load(Ordering::Relaxed) < self.total_size {
            let current_size = self.current_size.load(Ordering::Relaxed);
            pb.set_length(self.total_size);
            pb.set_position(current_size);
            let elapsed = std::time::Instant::now()
                .duration_since(self.start_time)
                .as_secs_f32();

            let downloaded_mb = current_size as f32 / 1_000_000.0;
            let speed = downloaded_mb / elapsed;
            pb.set_message(format!("{speed:.2} Mb/s"));
            speeds.send(speed).await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        pb.finish_and_clear();

        Ok(())
    }
}