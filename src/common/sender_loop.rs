use std::{
	thread,
	sync::Arc,
	time::{Duration, Instant}
};

use parking_lot::RwLock;

pub trait BufferSender
{
	fn send_buffered(&mut self);
}

const TICK_COUNT: usize = 30;

pub fn sender_loop<B: BufferSender>(sender: Arc<RwLock<B>>)
{
	let frame_duration = Duration::from_secs_f64(1.0 / TICK_COUNT as f64);
	let mut last_tick = Instant::now();

	loop
	{
		sender.write().send_buffered();

		if let Some(time) = frame_duration.checked_sub(last_tick.elapsed())
		{
			thread::sleep(time);
		}

		last_tick = Instant::now();
	}
}