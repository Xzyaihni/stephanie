use slab::Slab;

use crate::common::{
	EntityType,
	BufferSender,
	EntityPasser,
	MessagePasser,
	message::{Message, MessageBuffer}
};


#[derive(Debug)]
pub struct PlayerInfo
{
	message_buffer: MessageBuffer,
	message_passer: MessagePasser
}

impl PlayerInfo
{
	pub fn new(message_buffer: MessageBuffer, message_passer: MessagePasser) -> Self
	{
		Self{message_buffer, message_passer}
	}

	pub fn set_message(&mut self, message: Message)
	{
		self.message_buffer.set_message(message);
	}

	pub fn send_blocking(&mut self, message: Message) -> Result<(), bincode::Error>
	{
		self.message_passer.send(&message)
	}

	pub fn clone_messager(&self) -> MessagePasser
	{
		self.message_passer.try_clone()
	}
}

#[derive(Debug)]
pub struct ConnectionsHandler
{
	connections: Slab<PlayerInfo>
}

impl ConnectionsHandler
{
	pub fn new(limit: usize) -> Self
	{
		let connections = Slab::with_capacity(limit);

		Self{connections}
	}

	pub fn remove_connection(&mut self, id: usize)
	{
		self.connections.remove(id);
	}

	pub fn connections_amount(&self) -> usize
	{
		self.connections.len()
	}

	pub fn connect(&mut self, player_info: PlayerInfo) -> usize
	{
		self.connections.insert(player_info)
	}

	pub fn get_mut(&mut self, id: usize) -> &mut PlayerInfo
	{
		self.connections.get_mut(id).unwrap()
	}
}

impl EntityPasser for ConnectionsHandler
{
	fn send_message(&mut self, message: Message)
	{
		let entity_type = message.entity_type();

		self.connections.iter_mut().filter(|(index, _)|
		{
			Some(EntityType::Player(*index)) != entity_type
		}).for_each(|(_, player_info)|
		{
			player_info.set_message(message.clone());
		});
	}
}

impl BufferSender for ConnectionsHandler
{
	fn send_buffered(&mut self) -> Result<(), bincode::Error>
	{
		self.connections.iter_mut().try_for_each(|(_, connection)|
		{
			connection.message_buffer.get_buffered().try_for_each(|message|
			{
				connection.message_passer.send(&message)
			})
		})
	}
}