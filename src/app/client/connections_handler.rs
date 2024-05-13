use crate::common::{
    BufferSender,
    EntityPasser,
    MessagePasser,
    ConnectionId,
    message::{Message, MessageBuffer}
};


#[derive(Debug)]
pub struct ConnectionsHandler
{
    message_buffer: MessageBuffer,
    message_passer: MessagePasser
}

impl ConnectionsHandler
{
    pub fn new(message_passer: MessagePasser) -> Self
    {
        let message_buffer = MessageBuffer::new();

        Self{message_buffer, message_passer}
    }

    pub fn send_blocking(&mut self, message: &Message) -> Result<(), bincode::Error>
    {
        self.message_passer.send_one(message)
    }

    pub fn receive_blocking(&mut self) -> Result<Option<Message>, bincode::Error>
    {
        self.message_passer.receive_one()
    }

    pub fn passer_clone(&self) -> MessagePasser
    {
        self.message_passer.try_clone()
    }
}

impl EntityPasser for ConnectionsHandler
{
    fn send_single(&mut self, _id: ConnectionId, _message: Message)
    {
        unimplemented!()
    }

    fn send_message(&mut self, message: Message)
    {
        self.message_buffer.set_message(message);
    }
}

impl BufferSender for ConnectionsHandler
{
    fn send_buffered(&mut self) -> Result<(), bincode::Error>
    {
        let buffer = self.message_buffer.get_buffered().collect::<Vec<_>>();

        self.message_passer.send_many(&buffer)
    }
}
