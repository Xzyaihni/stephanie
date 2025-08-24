use std::collections::VecDeque;

use crate::common::{
    World,
    world::GlobalPos,
    message::Message
};


pub struct MessageThrottlerInfo
{
    pub chunk_sync_every: usize
}

struct PerXFrames
{
    per: usize,
    wait: usize
}

impl PerXFrames
{
    pub fn new(per: usize) -> Self
    {
        Self{per, wait: 0}
    }

    pub fn advance(&mut self)
    {
        if self.wait > 0
        {
            self.wait -= 1;
        }
    }

    pub fn receive(&mut self) -> bool
    {
        let status = self.wait == 0;

        if status
        {
            self.wait = self.per;
        }

        status
    }
}

pub struct MessageThrottler
{
    chunks_buffered: VecDeque<(GlobalPos, Message)>,
    chunk_sync_every: PerXFrames
}

impl MessageThrottler
{
    pub fn new(info: MessageThrottlerInfo) -> Self
    {
        Self{
            chunks_buffered: VecDeque::new(),
            chunk_sync_every: PerXFrames::new(info.chunk_sync_every)
        }
    }

    pub fn advance(&mut self)
    {
        self.chunk_sync_every.advance();
    }

    pub fn poll(&mut self, world: &World) -> Option<Message>
    {
        self.chunks_buffered.pop_front().and_then(|(_pos, x)| self.process(world, x))
    }

    pub fn process(&mut self, world: &World, message: Message) -> Option<Message>
    {
        match message
        {
            Message::EntityRemoveManyChunk{pos, ..} =>
            {
                if let Some(index) = self.chunks_buffered.iter().position(|(chunk_pos, value)|
                {
                    *chunk_pos == pos && matches!(value, Message::ChunkSync{..})
                })
                {
                    self.chunks_buffered.remove(index);
                    None
                } else
                {
                    Some(message)
                }
            },
            Message::ChunkSync{pos, ..} =>
            {
                if !world.inbounds(pos)
                {
                    return None;
                }

                if self.chunk_sync_every.receive()
                {
                    Some(message)
                } else
                {
                    self.chunks_buffered.push_back((pos, message));
                    None
                }
            },
            x => Some(x)
        }
    }
}
