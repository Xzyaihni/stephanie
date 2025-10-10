use crate::common::{
    MessageSerError,
    MessageDeError,
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

    pub fn send_blocking(&mut self, message: &Message) -> Result<(), MessageSerError>
    {
        self.message_passer.send_one(message)
    }

    pub fn receive_blocking(&mut self) -> Result<Option<Message>, MessageDeError>
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
    fn send_buffered(&mut self) -> Result<(), MessageSerError>
    {
        self.message_passer.send_many(self.message_buffer.buffered())?;

        self.message_buffer.clear_buffered();

        Ok(())
    }
}

impl EntityPasser for ()
{
    fn send_single(&mut self, _id: ConnectionId, _message: Message) {}

    fn send_message(&mut self, _message: Message) {}
}
