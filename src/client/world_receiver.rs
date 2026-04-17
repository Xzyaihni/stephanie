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
    buffered: VecDeque<GlobalPos>
}

impl ChunkWorldReceiver
{
    pub fn new() -> Self
    {
        Self{buffered: VecDeque::new()}
    }

    pub fn update(
        &mut self,
        passer: &mut ConnectionsHandler,
        indexer: &impl OvermapIndexing,
        is_loading: bool
    )
    {
        let mut take_amount = if is_loading
        {
            usize::MAX
        } else
        {
            2
        };

        while take_amount > 0
        {
            if let Some(pos) = self.buffered.pop_front()
            {
                if indexer.inbounds(pos)
                {
                    self.request_chunk_inner(passer, pos);

                    take_amount -= 1;
                }
            } else
            {
                break;
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
