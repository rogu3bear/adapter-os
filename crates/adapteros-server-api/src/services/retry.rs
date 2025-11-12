use tokio::time::{sleep, Duration};
use adapteros_deterministic_exec::GlobalSeed;
use rand::RngCore;
use tracing::warn;

pub async fn exponential_backoff<T, E, F, Fut>(max_attempts: usize, initial_delay: Duration, operation: F) -> Result<T, E> where F: Fn(usize) -> Fut, Fut: Future<Output = Result<T, E>> {
    let seed = GlobalSeed::get_or_init(b"retry_seed"); // Deterministic seed
    let mut rng = seed.rng();
    let mut attempt = 0;
    let mut delay = initial_delay;

    loop {
        attempt += 1;
        match operation(attempt).await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < max_attempts => {
                let jitter = rng.next_u32() as f64 / u32::MAX as f64 * 0.1 * delay.as_millis() as f64;
                let sleep_delay = delay + Duration::from_millis(jitter as u64);
                warn!("Attempt {} failed, retrying in {:?}", attempt, sleep_delay);
                sleep(sleep_delay).await;
                delay *= 2; // Backoff multiplier 2.0
                if delay > Duration::from_secs(1) { delay = Duration::from_secs(1); } // Max 1s
            }
            Err(e) => return Err(e),
        }
    }
}
