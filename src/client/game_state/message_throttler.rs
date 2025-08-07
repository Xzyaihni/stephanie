use crate::common::message::Message;


pub struct MessageThrottlerInfo
{
    pub max_entity_sets: usize
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
    entity_sets: CountLimited
}

impl MessageThrottler
{
    pub fn new(info: MessageThrottlerInfo) -> Self
    {
        Self{
            available: true,
            entity_sets: CountLimited::new(info.max_entity_sets)
        }
    }

    fn reset(&mut self)
    {
        self.available = true;

        self.entity_sets.reset();
    }

    pub fn process(&mut self, message: &Message)
    {
        let available = match message
        {
            Message::EntitySet{..} => self.entity_sets.add(),
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
