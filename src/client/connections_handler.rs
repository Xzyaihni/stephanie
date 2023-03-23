use crate::common::{
	BufferSender,
	EntityPasser,
	MessagePasser,
	message::{Message, MessageBuffer}
};


#[derive(Debug)]
pub struct ConnectionsHandler
{
	message_buffer: MessageBuffer,
	message_passer: MessagePasser
}

impl ConnectionsHandler
{
	pub fn new(message_passer: MessagePasser) -> Self
	{
		let message_buffer = MessageBuffer::new();

		Self{message_buffer, message_passer}
	}

	pub fn send(&mut self, message: &Message) -> Result<(), bincode::Error>
	{
		self.message_passer.send(message)
	}

	pub fn receive(&mut self) -> Result<Message, bincode::Error>
	{
		self.message_passer.receive()
	}

	pub fn passer_clone(&self) -> MessagePasser
	{
		self.message_passer.try_clone()
	}
}

impl EntityPasser for ConnectionsHandler
{
	fn send_message(&mut self, message: Message)
	{
		self.message_buffer.set_message(message);
	}
}

impl BufferSender for ConnectionsHandler
{
	fn send_buffered(&mut self) -> Result<(), bincode::Error>
	{
		self.message_buffer.get_buffered().try_for_each(|message|
		{
			self.message_passer.send(&message)
		})
	}
}