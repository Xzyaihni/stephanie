use crate::common::message::Message;


pub struct MessageThrottlerInfo
{
    pub max_chunk_syncs: usize
}

struct CountLimited
{
    max: usize,
    current: usize
}

impl CountLimited
{
    pub fn new(max: usize) -> Self
    {
        Self{max, current: max}
    }

    pub fn add(&mut self) -> bool
    {
        self.current += 1;

        self.current < self.max
    }

    pub fn reset(&mut self)
    {
        self.current = 0;
    }
}

pub struct MessageThrottler
{
    available: bool,
    chunk_syncs: CountLimited
}

impl MessageThrottler
{
    pub fn new(info: MessageThrottlerInfo) -> Self
    {
        Self{
            available: true,
            chunk_syncs: CountLimited::new(info.max_chunk_syncs)
        }
    }

    fn reset(&mut self)
    {
        self.available = true;

        self.chunk_syncs.reset();
    }

    pub fn process(&mut self, message: &Message)
    {
        let available = match message
        {
            Message::ChunkSync{..} => self.chunk_syncs.add(),
            _ => true
        };

        self.available &= available;
    }

    pub fn available(&mut self) -> bool
    {
        let is_available = self.available;

        if !is_available
        {
            self.reset();
        }

        is_available
    }
}
