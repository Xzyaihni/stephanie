use std::{
	sync::Arc
};

use parking_lot::RwLock;

use crate::{
	client::ConnectionsHandler,
	common::{
		EntityPasser,
		message::Message,
		world::chunk::GlobalPos
	}
};


#[derive(Debug)]
pub struct WorldReceiver
{
	message_handler: Arc<RwLock<ConnectionsHandler>>
}

impl WorldReceiver
{
	pub fn new(message_handler: Arc<RwLock<ConnectionsHandler>>) -> Self
	{
		Self{message_handler}
	}

	pub fn request_chunk(&self, pos: GlobalPos)
	{
		self.message_handler.write().send_message(Message::ChunkRequest{pos});
	}
}