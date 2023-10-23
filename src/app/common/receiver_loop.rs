use std::thread;

use crate::common::{
	MessagePasser,
	message::Message
};


pub fn receiver_loop<F, D>(mut messager: MessagePasser, mut on_message: F, on_close: D)
where
	F: FnMut(Message) + Send + 'static,
	D: FnOnce() + Send + 'static
{
	thread::spawn(move ||
	{
		loop
		{
			if let Ok(messages) = messager.receive()
			{
				messages.into_iter().for_each(|message| on_message(message));
			} else
			{
				on_close();
				return;
			}
		}
	});
}
