use repo::Repo;
use std::sync::{Arc, atomic::Ordering};
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub fn ticker(repo: Arc<Repo>,
              producer: SyncSender<Arc<Repo>>) -> Option<JoinHandle<()>> {
    let interval = repo.config().interval;

    if let Some(interval) = interval {
        let producer = producer.clone();

        return Some(thread::spawn(move || {
            use super::RUNNING;
            while RUNNING.load(Ordering::SeqCst) {
                // TODO: Calculate sleep time instead of checking regulary

                // Check if update is outstanding and send it as task to the worker
                if repo.last_updated().map_or(true, |t| t + interval < Instant::now()) {
                    producer.send(repo.clone()).unwrap();
                }

                // Delay
                thread::sleep(Duration::from_secs(1));
            }
        }));
    } else {
        return None;
    }
}
