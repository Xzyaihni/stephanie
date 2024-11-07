use std::{
    f32,
    fmt,
    mem,
    thread::JoinHandle,
    ops::ControlFlow,
    net::TcpStream,
    sync::{
        Arc,
        mpsc::{self, Sender, Receiver, TryRecvError}
    }
};

use parking_lot::RwLock;

use nalgebra::Vector3;

use yanyaengine::Transform;

use super::{
    ConnectionsHandler,
    connections_handler::PlayerInfo,
    world::World
};

pub use super::world::ParseError;

use crate::{
    debug_config::*,
    common::{
        some_or_return,
        sender_loop,
        receiver_loop,
        ENTITY_SCALE,
        render_info::*,
        lazy_transform::*,
        physics::*,
        MessageSerError,
        MessageDeError,
        AnyEntities,
        TileMap,
        DataInfos,
        Inventory,
        Entity,
        EntityInfo,
        Faction,
        CharactersInfo,
        CharacterId,
        Character,
        Player,
        Entities,
        Anatomy,
        HumanAnatomy,
        HumanAnatomyInfo,
        EntityPasser,
        EntitiesController,
        MessagePasser,
        ConnectionId,
        message::{
            Message,
            MessageBuffer
        }
    }
};


#[derive(Debug)]
pub enum ConnectionError
{
    MessageSerError(MessageSerError),
    MessageDeError(MessageDeError),
    ReceiverError(TryRecvError),
    WrongConnectionMessage
}

impl fmt::Display for ConnectionError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self
        {
            Self::MessageSerError(x) => x.to_string(),
            Self::MessageDeError(x) => x.to_string(),
            Self::ReceiverError(x) => x.to_string(),
            Self::WrongConnectionMessage => "wrong connection message".to_owned()
        };

        write!(f, "{s}")
    }
}

impl From<TryRecvError> for ConnectionError
{
    fn from(value: TryRecvError) -> Self
    {
        ConnectionError::ReceiverError(value)
    }
}

impl From<MessageSerError> for ConnectionError
{
    fn from(value: MessageSerError) -> Self
    {
        ConnectionError::MessageSerError(value)
    }
}

/*impl From<MessageDeError> for ConnectionError
{
    fn from(value: MessageDeError) -> Self
    {
        ConnectionError::MessageDeError(value)
    }
}*/

pub struct GameServer
{
    entities: Entities,
    player_character: CharacterId,
    characters_info: Arc<CharactersInfo>,
    world: World,
    sender: Sender<(ConnectionId, Message, Entity)>,
    receiver: Receiver<(ConnectionId, Message, Entity)>,
    connection_receiver: Receiver<TcpStream>,
    connection_handler: Arc<RwLock<ConnectionsHandler>>,
    receiver_handles: Vec<JoinHandle<()>>,
    exited: bool,
    rare_timer: f32
}

impl Drop for GameServer
{
    fn drop(&mut self)
    {
        self.world.exit(&mut self.entities);

        mem::take(&mut self.receiver_handles).into_iter().for_each(|receiver_handle|
        {
            receiver_handle.join().unwrap()
        });

        eprintln!("server shut down properly");
    }
}

impl GameServer
{
    pub fn new(
        tilemap: TileMap,
        data_infos: DataInfos,
        limit: usize
    ) -> Result<(Sender<TcpStream>, Self), ParseError>
    {
        let entities = Entities::new(data_infos.clone());
        let connection_handler = Arc::new(RwLock::new(ConnectionsHandler::new(limit)));

        let world = World::new(
            connection_handler.clone(),
            tilemap,
            data_infos.enemies_info.clone(),
            data_infos.items_info.clone()
        )?;

        let _sender_handle = sender_loop(connection_handler.clone());

        let (sender, receiver) = mpsc::channel();

        let (connector, connection_receiver) = mpsc::channel();

        Ok((connector, Self{
            entities,
            player_character: data_infos.player_character,
            characters_info: data_infos.characters_info,
            world,
            sender,
            receiver,
            connection_receiver,
            connection_handler,
            receiver_handles: Vec::new(),
            exited: false,
            rare_timer: 0.0
        }))
    }

    pub fn update(&mut self, dt: f32) -> bool
    {
        self.process_messages();

        self.entities.update_sprites(&self.characters_info);

        {
            let mut writer = self.connection_handler.write();
            self.entities.create_queued(&mut writer);
        }

        self.entities.update_watchers(dt);

        if self.rare_timer <= 0.0
        {
            self.rare();

            self.rare_timer = 5.0;
        } else
        {
            self.rare_timer -= dt;
        }

        self.exited
    }

    fn rare(&mut self)
    {
        if DebugConfig::is_debug()
        {
            self.entities.check_guarantees();
        }
    }

    fn process_connecting(&mut self) -> Result<(), ConnectionError>
    {
        loop
        {
            match self.connection_receiver.try_recv()
            {
                Ok(stream) =>
                {
                    self.connect(stream)?;
                },
                Err(TryRecvError::Empty) =>
                {
                    return Ok(());
                },
                Err(err) =>
                {
                    return Err(err.into());
                }
            }
        }
    }

    pub fn process_messages(&mut self)
    {
        if let Err(err) = self.process_connecting()
        {
            eprintln!("error connecting: {err}");
        }

        loop
        {
            match self.receiver.try_recv()
            {
                Ok((id, message, player_entities)) =>
                {
                    self.process_message_inner(message, id, player_entities);
                },
                Err(TryRecvError::Empty) =>
                {
                    return;
                },
                Err(err) =>
                {
                    eprintln!("error reading message: {err}");
                    return;
                }
            }
        }
    }

    fn exit(&mut self)
    {
        self.exited = true;
    }

    pub fn connect(&mut self, stream: TcpStream) -> Result<(), ConnectionError>
    {
        if self.connection_handler.read().under_limit()
        {
            self.player_connect(stream)
        } else
        {
            Ok(())
        }
    }

    pub fn player_connect(
        &mut self,
        stream: TcpStream
    ) -> Result<(), ConnectionError>
    {
        let (entity, id, messager) = self.player_connect_inner(stream)?;

        let sender0 = self.sender.clone();
        let sender1 = self.sender.clone();

        let receiver_handle = receiver_loop(
            messager,
            move |message|
            {
                let is_disconnect = match message
                {
                    Message::PlayerDisconnect{..} => true,
                    _ => false
                };

                if sender0.send((id, message, entity)).is_err() || is_disconnect
                {
                    ControlFlow::Break(())
                } else
                {
                    ControlFlow::Continue(())
                }
            },
            move ||
            {
                let _ = sender1.send((id, Message::PlayerDisconnect{host: false}, entity));
            }
        );

        self.receiver_handles.push(receiver_handle);

        Ok(())
    }

    fn player_connect_inner(
        &mut self,
        stream: TcpStream
    ) -> Result<(Entity, ConnectionId, MessagePasser), ConnectionError>
    {
        let player_index = self.entities.player.len() + 1;

        let transform = Transform{
            scale: Vector3::repeat(ENTITY_SCALE),
            ..Default::default()
        };

        let base_health = 1.2;
        let anatomy = Anatomy::Human(HumanAnatomy::new(HumanAnatomyInfo{
            bone_toughness: base_health,
            muscle_toughness: base_health,
            skin_toughness: base_health,
            base_speed: 0.9,
            ..Default::default()
        }));

        let position = transform.position;

        let info = EntityInfo{
            player: Some(Player),
            named: Some(format!("stephanie #{player_index}")),
            lazy_transform: Some(LazyTransformInfo{
                transform: transform.clone(),
                ..Default::default()
            }.into()),
            render: Some(RenderInfo{
                object: Some(RenderObjectKind::Texture{
                    name: "player/hair.png".to_owned()
                }.into()),
                z_level: ZLevel::Head,
                aspect: Aspect::KeepMax,
                ..Default::default()
            }),
            physical: Some(PhysicalProperties{
                inverse_mass: 50.0_f32.recip(),
                static_friction: 0.9,
                dynamic_friction: 0.8,
                fixed: PhysicalFixed{rotation: true, ..Default::default()},
                can_sleep: false,
                ..Default::default()
            }.into()),
            inventory: Some(Inventory::new()),
            character: Some(Character::new(self.player_character, Faction::Player)),
            anatomy: Some(anatomy),
            ..Default::default()
        };

        let mut inserter = |info: EntityInfo|
        {
            let inserted = self.entities.push_eager(false, info);

            let info = self.entities.info(inserted);

            let message = Message::EntitySet{entity: inserted, info};
            self.connection_handler.write().send_message(message);

            inserted
        };

        let player_entity = inserter(info);

        let player_info = self.player_info(stream, player_entity)?;

        let (connection, mut messager) = self.player_create(
            player_entity,
            player_info,
            position
        )?;

        messager.send_one(&Message::PlayerFullyConnected)?;

        Ok((player_entity, connection, messager))
    }

    fn player_info(&self, stream: TcpStream, entity: Entity) -> Result<PlayerInfo, ConnectionError>
    {
        let mut message_passer = MessagePasser::new(stream);

        let name = match message_passer.receive_one()?
        {
            Some(Message::PlayerConnect{name}) => name,
            _ =>
            {
                return Err(ConnectionError::WrongConnectionMessage);
            }
        };

        println!("player \"{name}\" connected");

        self.entities.named_mut(entity).unwrap().clone_from(&name);

        Ok(PlayerInfo::new(MessageBuffer::new(), message_passer, entity, name))
    }

    fn player_create(
        &mut self,
        player_entity: Entity,
        mut player_info: PlayerInfo,
        position: Vector3<f32>
    ) -> Result<(ConnectionId, MessagePasser), ConnectionError>
    {
        player_info.send_blocking(Message::PlayerOnConnect{player_entity})?;

        let connection_id = self.connection_handler.write().connect(player_info);

        self.world.add_player(
            &mut self.entities,
            connection_id,
            position.into()
        );

        crate::time_this!{
            "world-gen",
            self.world.send_all(&mut self.entities, connection_id)
        };

        let mut writer = self.connection_handler.write();
        writer.flush()?;

        let messager = writer.get_mut(connection_id);

        self.entities.try_for_each_entity(|entity|
        {
            if entity.local()
            {
                return Ok(());
            }

            let info = self.entities.info(entity);
            let message = Message::EntitySet{entity, info};

            messager.send_blocking(message)
        })?;

        Ok((connection_id, messager.clone_messager()))
    }

    fn connection_close(&mut self, host: bool, id: ConnectionId, entity: Entity)
    {
        let removed = self.connection_handler.write().remove_connection(id);

        self.world.remove_player(&mut self.entities, id);

        if host
        {
            self.exit();
        }

        let removed_name = removed.as_ref().map(|x| x.name().to_owned());

        if let Some(mut removed) = removed
        {
            if let Err(err) = removed.send_blocking(Message::PlayerDisconnectFinished)
            {
                eprintln!("error while disconnecting: {err}");
            }
        }

        if let Some(removed_name) = removed_name
        {
            println!("player \"{removed_name}\" disconnected");
        }

        let mut writer = self.connection_handler.write();
        writer.send_message(self.entities.remove_message(entity));
    }

    fn process_message_inner(
        &mut self,
        message: Message,
        id: ConnectionId,
        entity: Entity
    )
    {
        let message = match message
        {
            Message::RepeatMessage{message} =>
            {
                self.send_message(*message);

                return;
            },
            x => x
        };

        if message.forward()
        {
            self.connection_handler.write().send_message_without(id, message.clone());
        }

        let message = some_or_return!{self.world.handle_message(
            &mut self.entities,
            id,
            entity,
            message
        )};

        let message = some_or_return!{self.entities.handle_message(message)};

        match message
        {
            Message::PlayerDisconnect{host} => self.connection_close(host, id, entity),
            x => panic!("unhandled message: {x:?}")
        }
    }

    fn send_message(&mut self, message: Message)
    {
        self.connection_handler.write().send_message(message);
    }
}

impl EntitiesController for GameServer
{
    type Container = Entities;
    type Passer = ConnectionsHandler;

    fn container_ref(&self) -> &Self::Container
    {
        &self.entities
    }

    fn container_mut(&mut self) -> &mut Self::Container
    {
        &mut self.entities
    }

    fn passer(&self) -> Arc<RwLock<Self::Passer>>
    {
        self.connection_handler.clone()
    }
}
