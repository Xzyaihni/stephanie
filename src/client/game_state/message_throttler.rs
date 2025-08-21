use std::collections::VecDeque;

use crate::common::{
    World,
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
    buffered: VecDeque<Message>,
    chunk_sync_every: PerXFrames
}

impl MessageThrottler
{
    pub fn new(info: MessageThrottlerInfo) -> Self
    {
        Self{
            buffered: VecDeque::new(),
            chunk_sync_every: PerXFrames::new(info.chunk_sync_every)
        }
    }

    pub fn advance(&mut self)
    {
        self.chunk_sync_every.advance();
    }

    pub fn poll(&mut self, world: &World) -> Option<Message>
    {
        self.buffered.pop_front().and_then(|x| self.process(world, x))
    }

    pub fn process(&mut self, world: &World, message: Message) -> Option<Message>
    {
        match message
        {
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
                    self.buffered.push_back(message);
                    None
                }
            },
            x => Some(x)
        }
    }
}
