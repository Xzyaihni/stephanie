use std::{
    f32,
    fmt,
    mem,
    io,
    fs::{self, File},
    path::PathBuf,
    rc::Rc,
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
        world::{TILE_SIZE, CHUNK_VISUAL_SIZE, Pos3},
        chunk_saver::{with_temp_save, load_compressed},
        message::{
            Message,
            MessageBuffer
        }
    }
};

#[allow(unused_imports)]
use crate::common::message::DebugMessage;


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

impl From<MessageDeError> for ConnectionError
{
    fn from(value: MessageDeError) -> Self
    {
        ConnectionError::MessageDeError(value)
    }
}

pub struct GameServer
{
    entities: Entities,
    data_infos: DataInfos,
    tilemap: Rc<TileMap>,
    world: Option<World>,
    world_name: String,
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
        if let Some(world) = self.world.as_mut()
        {
            world.exit(&mut self.entities);
        }

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
        let tilemap = Rc::new(tilemap);
        let entities = Entities::new(data_infos.clone());
        let connection_handler = Arc::new(RwLock::new(ConnectionsHandler::new(limit)));

        let world_name = "default".to_owned();

        let world = Some(World::new(
            connection_handler.clone(),
            tilemap.clone(),
            data_infos.clone(),
            world_name.clone()
        )?);

        let _sender_handle = sender_loop(connection_handler.clone());

        let (sender, receiver) = mpsc::channel();

        let (connector, connection_receiver) = mpsc::channel();

        Ok((connector, Self{
            entities,
            data_infos,
            tilemap,
            world,
            world_name,
            sender,
            receiver,
            connection_receiver,
            connection_handler,
            receiver_handles: Vec::new(),
            exited: false,
            rare_timer: 0.0
        }))
    }

    fn restart(&mut self)
    {
        self.entities = Entities::new(self.data_infos.clone());

        self.world.take();

        {
            let path = PathBuf::from("worlds").join(&self.world_name);
            if path.exists()
            {
                eprintln!("removing {}", path.display());

                if let Err(err) = fs::remove_dir_all(&path)
                {
                    eprintln!("error removing {}: {err}", path.display());
                }
            }
        }

        self.world = Some(World::new(
            self.connection_handler.clone(),
            self.tilemap.clone(),
            self.data_infos.clone(),
            self.world_name.clone()
        ).unwrap());
    }

    pub fn update(&mut self, dt: f32) -> bool
    {
        self.process_messages();

        self.entities.update_sprites(&self.data_infos.characters_info);

        {
            let mut writer = self.connection_handler.write();
            self.entities.create_queued(&mut writer);
        }

        self.entities.update_watchers(dt);

        self.entities.resort_queued();

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
                let _ = sender1.send((id, Message::PlayerDisconnect{restart: false, host: false}, entity));
            }
        );

        self.receiver_handles.push(receiver_handle);

        Ok(())
    }

    fn load_player_info(&self, player_name: &str) -> Option<EntityInfo>
    {
        let path = self.player_info_path(player_name);

        let file = match File::open(&path)
        {
            Ok(x) => Some(x),
            Err(ref err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) =>
            {
                eprintln!("error trying to open \"{player_name}\" save file: {err}");

                None
            }
        };

        let info = file.and_then(|file|
        {
            match load_compressed(file)
            {
                Ok(x) => Some(x),
                Err(err) =>
                {
                    eprintln!("error trying to load player \"{player_name}\": {err}");

                    None
                }
            }
        });

        if info.is_some()
        {
            eprintln!("loading player \"{player_name}\"");
        }

        info
    }

    fn create_new_player(&self, name: String) -> EntityInfo
    {
        let transform = {
            let scale = Vector3::repeat(ENTITY_SCALE);

            let mut position = Vector3::repeat(CHUNK_VISUAL_SIZE / 2.0);
            position.z = -TILE_SIZE + (scale.z / 2.0);

            Transform{
                position,
                scale,
                ..Default::default()
            }
        };

        let base_health = 0.6;
        let anatomy = Anatomy::Human(HumanAnatomy::new(HumanAnatomyInfo{
            bone_toughness: base_health,
            muscle_toughness: base_health,
            skin_toughness: base_health,
            base_speed: 0.9,
            base_strength: 1.0,
            ..Default::default()
        }));

        EntityInfo{
            player: Some(Player::default()),
            named: Some(name),
            lazy_transform: Some(LazyTransformInfo{
                transform: transform.clone(),
                ..Default::default()
            }.into()),
            render: Some(RenderInfo{
                aspect: Aspect::KeepMax,
                ..Default::default()
            }),
            physical: Some(PhysicalProperties{
                inverse_mass: 50.0_f32.recip(),
                fixed: PhysicalFixed{rotation: true, ..Default::default()},
                can_sleep: false,
                ..Default::default()
            }.into()),
            inventory: Some(Inventory::new()),
            character: Some(Character::new(self.data_infos.player_character, Faction::Player)),
            anatomy: Some(anatomy),
            ..Default::default()
        }
    }

    fn player_connect_inner(
        &mut self,
        stream: TcpStream
    ) -> Result<(Entity, ConnectionId, MessagePasser), ConnectionError>
    {
        let mut player_info = self.player_info(stream)?;

        let info = self.load_player_info(&player_info.name).unwrap_or_else(||
        {
            self.create_new_player(player_info.name.clone())
        });

        let player_position = info.transform.as_ref().map(|x| x.position).or_else(||
        {
            info.lazy_transform.as_ref().map(|x| x.target_local.position)
        }).unwrap_or_else(Vector3::zeros);

        let mut inserter = |info: EntityInfo|
        {
            let inserted = self.entities.push_eager(false, info);

            let info = self.entities.info(inserted);

            let message = Message::EntitySet{entity: inserted, info: Box::new(info)};
            self.connection_handler.write().send_message(message);

            inserted
        };

        let player_entity = inserter(info);

        player_info.entity = Some(player_entity);

        let (connection, mut messager) = self.player_create(player_info, player_position)?;

        messager.send_one(&Message::PlayerFullyConnected)?;

        Ok((player_entity, connection, messager))
    }

    fn player_info(&self, stream: TcpStream) -> Result<PlayerInfo, ConnectionError>
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

        Ok(PlayerInfo{message_buffer: MessageBuffer::new(), message_passer, entity: None, name})
    }

    fn player_create(
        &mut self,
        mut player_info: PlayerInfo,
        position: Vector3<f32>
    ) -> Result<(ConnectionId, MessagePasser), ConnectionError>
    {
        let player_position = Pos3::from(position);
        player_info.send_blocking(Message::PlayerOnConnect{player_entity: player_info.entity.unwrap(), player_position})?;

        let connection_id = self.connection_handler.write().connect(player_info);

        if DebugConfig::is_enabled(DebugTool::LoadPosition)
        {
            eprintln!("server {connection_id:?}: {player_position}");
        }

        self.world.as_mut().unwrap().add_player(
            &mut self.entities,
            connection_id,
            position.into()
        );

        self.world.as_mut().unwrap().sync_camera(&mut self.entities, connection_id, player_position);

        crate::time_this!{
            "world-gen",
            self.world.as_mut().unwrap().send_all(&mut self.entities, connection_id)
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
            let message = Message::EntitySet{entity, info: Box::new(info)};

            messager.send_blocking(message)
        })?;

        Ok((connection_id, messager.clone_messager()))
    }

    fn connection_close(&mut self, restart: bool, host: bool, id: ConnectionId, entity: Entity)
    {
        let removed = self.connection_handler.write().remove_connection(id);

        self.world.as_mut().unwrap().remove_player(&mut self.entities, id);

        if !restart && host
        {
            self.exit();
        }

        if let Some(mut removed) = removed
        {
            let player_name = removed.name().to_owned();
            println!("player \"{player_name}\" disconnected");

            if !restart
            {
                if let Some(player_entity) = removed.entity
                {
                    let player_info = self.entities.info(player_entity);

                    println!("saving player \"{player_name}\"");

                    let path = self.player_info_path(&player_name);

                    let world_directory = path.parent().expect("player path must not be empty");

                    if let Err(err) = fs::create_dir_all(world_directory)
                    {
                        eprintln!("error trying to create world directory: {err}");
                    }

                    if let Err(err) = File::create(&path)
                    {
                        eprintln!("error trying to create player save file: {err}");
                    }

                    if let Err(err) = with_temp_save(path, player_info)
                    {
                        eprintln!("error trying to save player: {err}");
                    }
                }
            }

            if let Err(err) = removed.send_blocking(Message::PlayerDisconnectFinished)
            {
                eprintln!("error while disconnecting: {err}");
            }
        }

        {
            let mut writer = self.connection_handler.write();
            writer.send_message(self.entities.remove_message(entity));
        }

        if restart
        {
            self.restart();
        }
    }

    fn player_info_path(&self, player_name: &str) -> PathBuf
    {
        let formatted_name = player_name.chars().map(|c|
        {
            if c.is_ascii_graphic()
            {
                c
            } else
            {
                char::REPLACEMENT_CHARACTER
            }
        }).collect::<String>();

        PathBuf::from("worlds").join(&self.world_name).join(format!("{formatted_name}.save"))
    }

    fn process_message_inner(
        &mut self,
        message: Message,
        id: ConnectionId,
        entity: Entity
    )
    {
        if let Message::RepeatMessage{message} = message
        {
            self.send_message(*message);

            return;
        }

        {
            let sync_transform = |entity: Entity, transform: Transform|
            {
                self.entities.set_transform(entity, Some(transform));
            };

            match &message
            {
                Message::SetTarget{entity, target} =>
                {
                    sync_transform(*entity, (**target).clone());
                },
                Message::SetLazyTransform{entity, component: Some(lazy)} =>
                {
                    let parent_transform = self.entities.parent_transform(*entity);
                    sync_transform(*entity, lazy.target_global(parent_transform.as_ref()));
                },
                _ => ()
            }
        }

        if message.forward()
        {
            self.connection_handler.write().send_message_without(id, message.clone());
        }

        let message = some_or_return!{self.world.as_mut().unwrap().handle_message(
            &mut self.entities,
            id,
            message
        )};

        let message = some_or_return!{self.entities.handle_message(message)};

        match message
        {
            Message::PlayerDisconnect{restart, host} => self.connection_close(restart, host, id, entity),
            #[cfg(debug_assertions)]
            Message::DebugMessage(DebugMessage::PrintEntityInfo(entity)) =>
            {
                eprintln!("server entity info: {}", self.entities.info_ref(entity))
            },
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
