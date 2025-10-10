use std::collections::VecDeque;

use crate::{
    client::ConnectionsHandler,
    common::{
        EntityPasser,
        message::Message,
        world::{GlobalPos, overmap::OvermapIndexing}
    }
};


#[derive(Debug, Clone)]
pub struct ChunkWorldReceiver
{
    delay: usize,
    buffered: VecDeque<GlobalPos>
}

impl ChunkWorldReceiver
{
    pub fn new() -> Self
    {
        Self{delay: 0, buffered: VecDeque::new()}
    }

    pub fn update(&mut self, passer: &mut ConnectionsHandler, indexer: &impl OvermapIndexing)
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
                    self.request_chunk_inner(passer, pos);

                    self.delay = 5;
                }
            }
        }
    }

    pub fn request_chunk(&mut self, pos: GlobalPos)
    {
        if !self.buffered.contains(&pos)
        {
            self.buffered.push_back(pos);
        }
    }

    fn request_chunk_inner(&self, passer: &mut ConnectionsHandler, pos: GlobalPos)
    {
        passer.send_message(Message::ChunkRequest{pos});
    }
}
