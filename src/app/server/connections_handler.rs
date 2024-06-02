use crate::common::{
    ObjectsStore,
    BufferSender,
    Entity,
    EntityPasser,
    MessagePasser,
    ConnectionId,
    message::{Message, MessageBuffer}
};


#[derive(Debug)]
pub struct PlayerInfo
{
    message_buffer: MessageBuffer,
    message_passer: MessagePasser,
    entity: Entity
}

impl PlayerInfo
{
    pub fn new(
        message_buffer: MessageBuffer,
        message_passer: MessagePasser,
        entity: Entity
    ) -> Self
    {
        Self{message_buffer, message_passer, entity}
    }

    pub fn set_message(&mut self, message: Message)
    {
        self.message_buffer.set_message(message);
    }

    pub fn send_blocking(&mut self, message: Message) -> Result<(), bincode::Error>
    {
        self.message_passer.send_one(&message)
    }

    pub fn clone_messager(&self) -> MessagePasser
    {
        self.message_passer.try_clone()
    }
}

#[derive(Debug)]
pub struct ConnectionsHandler
{
    connections: ObjectsStore<PlayerInfo>,
    limit: usize
}

impl ConnectionsHandler
{
    pub fn new(limit: usize) -> Self
    {
        let connections = ObjectsStore::with_capacity(limit);

        Self{connections, limit}
    }

    pub fn remove_connection(&mut self, id: ConnectionId) -> Option<PlayerInfo>
    {
        let mut removed = self.connections.remove(id.0);

        if let Some(removed) = removed.as_mut()
        {
            removed.message_buffer.clear();
        }

        removed
    }

    pub fn under_limit(&self) -> bool
    {
        self.connections.len() < self.limit
    }

    pub fn connect(&mut self, player_info: PlayerInfo) -> ConnectionId
    {
        ConnectionId(self.connections.push(player_info))
    }

    pub fn get(&self, id: ConnectionId) -> &PlayerInfo
    {
        self.connections.get(id.0).unwrap()
    }

    pub fn get_mut(&mut self, id: ConnectionId) -> &mut PlayerInfo
    {
        self.connections.get_mut(id.0).unwrap()
    }

    pub fn send_message_without(&mut self, id: ConnectionId, message: Message)
    {
        self.send_message_inner(Some(id), message);
    }

    fn send_message_inner(&mut self, skip_id: Option<ConnectionId>, message: Message)
    {
        let entity_type = message.entity();

        self.connections.iter_mut().filter(|(index, player_info)|
        {
            let same_sync = Some(player_info.entity) == entity_type;

            !same_sync && skip_id != Some(ConnectionId(*index))
        }).for_each(|(_, player_info)|
        {
            player_info.set_message(message.clone());
        });
    }
}

impl EntityPasser for ConnectionsHandler
{
    fn send_single(&mut self, id: ConnectionId, message: Message)
    {
        self.connections[id.0].set_message(message);
    }

    fn send_message(&mut self, message: Message)
    {
        self.send_message_inner(None, message);
    }
}

impl BufferSender for ConnectionsHandler
{
    fn send_buffered(&mut self) -> Result<(), bincode::Error>
    {
        self.connections.iter_mut().try_for_each(|(_, connection)|
        {
            let buffer = connection.message_buffer.get_buffered();

            connection.message_passer.send_many(&buffer)
        })
    }
}
