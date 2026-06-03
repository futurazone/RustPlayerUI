use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Persistent background image loader with a fixed worker pool.
///
/// Workers pop paths from a shared FIFO queue, decode them with `image::open`,
/// and send the RGBA pixels through the channel. The main thread timer tick
/// receives and inserts into the Slint model (4/frame).
pub struct ImageLoader {
    queue: Arc<Mutex<VecDeque<String>>>,
    workers: Vec<JoinHandle<()>>,
    shutdown: Arc<Mutex<bool>>,
}

impl ImageLoader {
    pub fn new(
        tx: mpsc::Sender<(String, u32, u32, Vec<u8>)>,
        num_workers: usize,
    ) -> Self {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let shutdown = Arc::new(Mutex::new(false));

        let mut workers = Vec::with_capacity(num_workers);
        for _ in 0..num_workers {
            let q = Arc::clone(&queue);
            let sd = Arc::clone(&shutdown);
            let tx = tx.clone();
            workers.push(std::thread::spawn(move || loop {
                if *sd.lock().unwrap() {
                    break;
                }
                let path = {
                    let mut guard = q.lock().unwrap();
                    guard.pop_front()
                };
                if let Some(path) = path {
                    log::debug!("Loader: Loading from disk: {}", path);
                    match image::open(&path) {
                        Ok(img) => {
                            let rgba = img.into_rgba8();
                            let (w, h) = (rgba.width(), rgba.height());
                            let pixels = rgba.into_raw();
                            let _ = tx.send((path, w, h, pixels));
                        }
                        Err(e) => {
                            log::error!("Loader: Failed to open {}: {}", path, e);
                        }
                    }
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }));
        }

        Self {
            queue,
            workers,
            shutdown,
        }
    }

    /// Enqueue a single path for loading (FIFO).
    pub fn enqueue(&self, path: String) {
        self.queue.lock().unwrap().push_back(path);
    }

    /// Enqueue multiple paths in FIFO order.
    pub fn enqueue_batch<I>(&self, paths: I)
    where
        I: IntoIterator<Item = String>,
    {
        let mut q = self.queue.lock().unwrap();
        for p in paths {
            q.push_back(p);
        }
    }
}

impl Drop for ImageLoader {
    fn drop(&mut self) {
        *self.shutdown.lock().unwrap() = true;
    }
}
