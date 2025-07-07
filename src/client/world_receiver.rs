use std::sync::Arc;

use parking_lot::RwLock;

use crate::{
    client::ConnectionsHandler,
    common::{
        EntityPasser,
        message::Message,
        world::{TilePos, Tile, GlobalPos}
    }
};


#[derive(Debug, Clone)]
pub struct WorldReceiver(Arc<RwLock<ConnectionsHandler>>);

impl WorldReceiver
{
    pub fn new(message_handler: Arc<RwLock<ConnectionsHandler>>) -> Self
    {
        Self(message_handler)
    }

    pub fn set_tile(&self, pos: TilePos, tile: Tile)
    {
        self.send_message(Message::SetTile{pos, tile});
    }

    pub fn request_chunk(&self, pos: GlobalPos)
    {
        self.send_message(Message::ChunkRequest{pos});
    }

    fn send_message(&self, message: Message)
    {
        self.0.write().send_message(message);
    }
}
