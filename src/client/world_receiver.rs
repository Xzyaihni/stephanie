use std::{
    sync::Arc,
    collections::VecDeque
};

use parking_lot::RwLock;

use crate::{
    client::ConnectionsHandler,
    common::{
        EntityPasser,
        message::Message,
        world::{TilePos, Tile, GlobalPos, overmap::OvermapIndexing}
    }
};


#[derive(Debug, Clone)]
pub struct ChunkWorldReceiver
{
    delay: usize,
    buffered: VecDeque<GlobalPos>,
    receiver: WorldReceiver
}

impl ChunkWorldReceiver
{
    pub fn new(receiver: WorldReceiver) -> Self
    {
        Self{delay: 0, buffered: VecDeque::new(), receiver}
    }

    pub fn update(&mut self, indexer: &impl OvermapIndexing)
    {
        if self.delay > 0
        {
            self.delay -= 1;
        } else
        {
            if let Some(pos) = self.buffered.pop_front()
            {
                if indexer.inbounds(pos)
                {
                    self.request_chunk_inner(pos);
                }
            }
        }
    }

    pub fn request_chunk(&mut self, pos: GlobalPos)
    {
        if self.delay == 0
        {
            self.request_chunk_inner(pos);

            self.delay = 5;
        } else
        {
            if !self.buffered.contains(&pos)
            {
                self.buffered.push_back(pos);
            }
        }
    }

    fn request_chunk_inner(&self, pos: GlobalPos)
    {
        self.receiver.send_message(Message::ChunkRequest{pos});
    }
}

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

    fn send_message(&self, message: Message)
    {
        self.0.write().send_message(message);
    }
}
