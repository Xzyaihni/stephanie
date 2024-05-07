use crate::common::{
    ObjectsStore,
	EntityId,
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
		self.message_passer.send_one(&message)
	}

	pub fn clone_messager(&self) -> MessagePasser
	{
		self.message_passer.try_clone()
	}
}

#[derive(Debug)]
pub struct ConnectionsHandler
{
	connections: ObjectsStore<PlayerInfo>,
	limit: usize
}

impl ConnectionsHandler
{
	pub fn new(limit: usize) -> Self
	{
		let connections = ObjectsStore::with_capacity(limit);

		Self{connections, limit}
	}

	pub fn remove_connection(&mut self, id: usize)
	{
		self.connections.remove(id);
	}

	pub fn under_limit(&self) -> bool
	{
		self.connections.len() < self.limit
	}

	pub fn connect(&mut self, player_info: PlayerInfo) -> usize
	{
		self.connections.push(player_info)
	}

	pub fn get_mut(&mut self, id: usize) -> &mut PlayerInfo
	{
		self.connections.get_mut(id).unwrap()
	}

    pub fn send_message_without(&mut self, id: usize, message: Message)
    {
        self.send_message_inner(Some(id), message);
    }

    fn send_message_inner(&mut self, skip_id: Option<usize>, message: Message)
    {
		let entity_type = message.entity_type();

		/*self.connections.iter_mut().filter(|(index, _)|
		{
			let same_sync = Some(EntityType::Player(*index)) == entity_type;

            !same_sync && skip_id != Some(*index)
		}).for_each(|(_, player_info)|
		{
			player_info.set_message(message.clone());
		});*/
        todo!();
    }
}

impl EntityPasser for ConnectionsHandler
{
	fn send_single(&mut self, id: usize, message: Message)
	{
		self.connections[id].set_message(message);
	}

	fn send_message(&mut self, message: Message)
	{
        self.send_message_inner(None, message);
	}
}

impl BufferSender for ConnectionsHandler
{
	fn send_buffered(&mut self) -> Result<(), bincode::Error>
	{
		self.connections.iter_mut().try_for_each(|(_, connection)|
		{
			let buffer = connection.message_buffer.get_buffered().collect::<Vec<_>>();

			connection.message_passer.send_many(&buffer)
		})
	}
}
