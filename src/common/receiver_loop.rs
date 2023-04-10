use std::{
	thread
};

use crate::common::{
	MessagePasser,
	message::Message
};


pub fn receiver_loop<T, FP, FD>(
	this: T,
	mut handler: MessagePasser,
	mut process_function: FP,
	disconnect_function: FD
)
where
	T: Clone + Send + 'static,
	FP: FnMut(T, Message) + Send + 'static,
	FD: FnOnce(T) + Send + 'static
{
	thread::spawn(move ||
	{
		loop
		{
			if let Ok(messages) = handler.receive()
			{
				messages.into_iter().for_each(|message|
				{
					process_function(this.clone(), message)
				});
			} else
			{
				disconnect_function(this);
				return;
			}
		}
	});
}