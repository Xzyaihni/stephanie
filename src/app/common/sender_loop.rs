use std::{
    thread::{self, JoinHandle},
    sync::Arc,
    time::{Duration, Instant}
};

use parking_lot::RwLock;

use crate::common::MessageSerError;

pub trait BufferSender
{
    fn send_buffered(&mut self) -> Result<(), MessageSerError>;
}

const TICK_COUNT: usize = 30;
pub const DELTA_TIME: f64 = 1.0 / TICK_COUNT as f64;

pub fn waiting_loop<F: FnMut() -> bool>(mut f: F)
{
    let frame_duration = Duration::from_secs_f64(DELTA_TIME);
    let mut last_tick = Instant::now();

    loop
    {
        if f() { return; }

        if let Some(time) = frame_duration.checked_sub(last_tick.elapsed())
        {
            thread::sleep(time);
        }

        last_tick = Instant::now();
    }
}

pub fn sender_loop<B: BufferSender + Send + Sync + 'static>(
    sender: Arc<RwLock<B>>
) -> JoinHandle<()>
{
    thread::spawn(move ||
    {
        waiting_loop(||
        {
            // error only happens if receiver hung up
            sender.write().send_buffered().is_err()
        });
    })
}
