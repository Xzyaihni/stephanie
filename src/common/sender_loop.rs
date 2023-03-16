use std::{
	thread,
	sync::Arc,
	time::{Duration, Instant}
};

use parking_lot::RwLock;

pub trait BufferSender
{
	fn send_buffered(&mut self) -> Result<(), bincode::Error>;
}

const TICK_COUNT: usize = 30;

pub fn sender_loop<B: BufferSender + Send + Sync + 'static>(sender: Arc<RwLock<B>>)
{
	thread::spawn(move ||
	{
		let frame_duration = Duration::from_secs_f64(1.0 / TICK_COUNT as f64);
		let mut last_tick = Instant::now();

		loop
		{
			if let Err(x) = sender.write().send_buffered()
			{
				eprintln!("error in sender loop: {x:?}, closing");
				return;
			}

			if let Some(time) = frame_duration.checked_sub(last_tick.elapsed())
			{
				thread::sleep(time);
			}

			last_tick = Instant::now();
		}
	});
}