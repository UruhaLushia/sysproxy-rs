use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::options::Options;

pub fn wait_proxy_settings_change(
    cancel: Arc<AtomicBool>,
    _opt: Option<&Options>,
) -> Result<()> {
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("cancelled"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
