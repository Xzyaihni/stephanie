use std::{
    f32,
    fmt,
    ops::ControlFlow,
    net::TcpStream,
    sync::Arc
};

use parking_lot::{RwLock, Mutex};

use nalgebra::Vector3;

use yanyaengine::Transform;

use super::{
    ConnectionsHandler,
    connections_handler::PlayerInfo,
    world::World
};

pub use super::world::ParseError;

use crate::common::{
    some_or_return,
    sender_loop,
    receiver_loop,
    ENTITY_SCALE,
    render_info::*,
    collider::*,
    AnyEntities,
    TileMap,
    DataInfos,
    Inventory,
    Entity,
    EntityInfo,
    Parent,
    Faction,
    CharactersInfo,
    CharacterId,
    Character,
    Player,
    PlayerEntities,
    Entities,
    Anatomy,
    HumanAnatomy,
    EntityPasser,
    EntitiesController,
    MessagePasser,
    ConnectionId,
    PhysicalProperties,
    lazy_transform::*,
    world::chunk::TILE_SIZE,
    message::{
        Message,
        MessageBuffer
    }
};


#[derive(Debug)]
pub enum ConnectionError
{
    BincodeError(bincode::Error),
    WrongConnectionMessage
}

impl fmt::Display for ConnectionError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self
        {
            Self::BincodeError(x) => x.to_string(),
            Self::WrongConnectionMessage => "wrong connection message".to_owned()
        };

        write!(f, "{s}")
    }
}

impl From<bincode::Error> for ConnectionError
{
    fn from(value: bincode::Error) -> Self
    {
        ConnectionError::BincodeError(value)
    }
}

pub struct GameServer
{
    entities: Entities,
    player_character: CharacterId,
    characters_info: Arc<CharactersInfo>,
    world: World,
    connection_handler: Arc<RwLock<ConnectionsHandler>>
}

impl GameServer
{
    pub fn new(
        tilemap: TileMap,
        data_infos: DataInfos,
        limit: usize
    ) -> Result<Self, ParseError>
    {
        let entities = Entities::new();
        let connection_handler = Arc::new(RwLock::new(ConnectionsHandler::new(limit)));

        let world = World::new(
            connection_handler.clone(),
            tilemap,
            data_infos.enemies_info.clone(),
            data_infos.items_info.clone()
        )?;

        sender_loop(connection_handler.clone());

        Ok(Self{
            entities,
            player_character: data_infos.player_character,
            characters_info: data_infos.characters_info,
            world,
            connection_handler
        })
    }

    pub fn update(&mut self, dt: f32)
    {
        const STEPS: u32 = 2;

        self.entities.update_sprites(&self.characters_info);

        for _ in 0..STEPS
        {
            let dt = (dt / STEPS as f32).min(0.1);

            self.entities.update_physical(dt);
            self.entities.update_lazy();
        }

        self.entities.update_watchers(dt);
        self.entities.create_queued();
    }

    pub fn connect(this: Arc<Mutex<Self>>, stream: TcpStream) -> Result<(), ConnectionError>
    {
        if this.lock().connection_handler.read().under_limit()
        {
            Self::player_connect(this, stream)
        } else
        {
            Ok(())
        }
    }

    pub fn player_connect(
        this: Arc<Mutex<Self>>,
        stream: TcpStream
    ) -> Result<(), ConnectionError>
    {
        let (entities, id, messager) = this.lock().player_connect_inner(stream)?;

        let entities0 = entities.clone();
        let entities1 = entities.clone();

        let other_this = this.clone();
        receiver_loop(
            messager,
            move |message|
            {
                this.lock().process_message_inner(message, id, entities0.clone());

                ControlFlow::Continue(())
            },
            move || other_this.lock().connection_close(false, id, entities1)
        );

        Ok(())
    }

    fn player_connect_inner(
        &mut self,
        stream: TcpStream
    ) -> Result<(PlayerEntities, ConnectionId, MessagePasser), ConnectionError>
    {
        let player_index = self.entities.player.len() + 1;

        let half_tile = TILE_SIZE / 2.0;

        let transform = Transform{
            scale: Vector3::repeat(ENTITY_SCALE),
            position: Vector3::new(0.0, 0.0, TILE_SIZE) + Vector3::repeat(half_tile),
            ..Default::default()
        };

        let physical = PhysicalProperties{
            mass: 50.0,
            friction: 0.99,
            floating: false
        };

        let anatomy = Anatomy::Human(HumanAnatomy::default());

        let position = transform.position;

        let info = EntityInfo{
            player: Some(Player{
                name: format!("stephanie #{player_index}"),
                strength: 1.0,
                holding: None
            }),
            transform: Some(transform.clone()),
            render: Some(RenderInfo{
                object: Some(RenderObject::Texture{
                    name: "player/hair.png".to_owned()
                }),
                shape: Some(BoundingShape::Circle),
                z_level: ZLevel::Head,
                ..Default::default()
            }),
            collider: Some(ColliderInfo{
                kind: ColliderType::Circle,
                ..Default::default()
            }.into()),
            inventory: Some(Inventory::new()),
            physical: Some(physical.into()),
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

        let inserted = inserter(info);

        let mut player_children = Vec::new();

        let held_item = |flip|
        {
            EntityInfo{
                render: Some(RenderInfo{
                    object: Some(RenderObject::Texture{
                        name: "placeholder.png".to_owned()
                    }),
                    flip: if flip { Uvs::FlipHorizontal } else { Uvs::Normal },
                    shape: Some(BoundingShape::Circle),
                    z_level: ZLevel::Arms,
                    ..Default::default()
                }),
                parent: Some(Parent::new(inserted, false)),
                lazy_transform: Some(LazyTransformInfo{
                    origin_rotation: -f32::consts::FRAC_PI_2,
                    transform: Transform{
                        rotation: f32::consts::FRAC_PI_2,
                        position: Vector3::new(1.0, 0.0, 0.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                watchers: Some(Default::default()),
                ..Default::default()
            }
        };

        let holding = inserter(held_item(true));
        let holding_right = inserter(held_item(false));

        let pon = |position: Vector3<f32>|
        {
            EntityInfo{
                lazy_transform: Some(LazyTransformInfo{
                    connection: Connection::Spring(
                        SpringConnection{
                            physical: PhysicalProperties{
                                mass: 0.01,
                                friction: 0.8,
                                floating: true
                            }.into(),
                            limit: 0.004,
                            damping: 0.02,
                            strength: 0.9
                        }
                    ),
                    rotation: Rotation::EaseOut(
                        EaseOutRotation{
                            decay: 25.0,
                            speed_significant: 3.0,
                            momentum: 0.5
                        }.into()
                    ),
                    deformation: Deformation::Stretch(
                        StretchDeformation{
                            animation: ValueAnimation::EaseOut(2.0),
                            limit: 1.3,
                            onset: 0.3,
                            strength: 0.5
                        }
                    ),
                    transform: Transform{
                        scale: Vector3::repeat(0.4),
                        position,
                        ..Default::default()
                    },
                    ..Default::default()
                }.into()),
                parent: Some(Parent::new(inserted, true)),
                render: Some(RenderInfo{
                    object: Some(RenderObject::Texture{
                        name: "player/pon.png".to_owned()
                    }),
                    shape: Some(BoundingShape::Circle),
                    z_level: ZLevel::Hair,
                    ..Default::default()
                }),
                ..Default::default()
            }
        };

        player_children.push(inserter(pon(Vector3::new(-0.35, 0.35, 0.0))));
        player_children.push(inserter(pon(Vector3::new(-0.35, -0.35, 0.0))));

        let player_info = self.player_info(stream, inserted)?;

        let player_entities = PlayerEntities{
            player: inserted,
            holding,
            holding_right,
            other: player_children
        };

        let (connection, mut messager) = self.player_create(
            player_entities.clone(),
            player_info,
            position
        )?;

        messager.send_one(&Message::PlayerFullyConnected)?;

        Ok((player_entities, connection, messager))
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

        Ok(PlayerInfo::new(MessageBuffer::new(), message_passer, entity, name))
    }

    fn player_create(
        &mut self,
        player_entities: PlayerEntities,
        player_info: PlayerInfo,
        position: Vector3<f32>
    ) -> Result<(ConnectionId, MessagePasser), ConnectionError>
    {
        let connection_id = self.connection_handler.write().connect(player_info);

        {
            let mut writer = self.connection_handler.write();

            let messager = writer.get_mut(connection_id);

            let message = Message::PlayerOnConnect{player_entities};

            messager.send_blocking(message)?;
        }

        self.world.add_player(
            &mut self.entities,
            connection_id,
            position.into()
        );

        self.world.send_all(&mut self.entities, connection_id);

        let mut writer = self.connection_handler.write();
        writer.flush()?;

        let messager = writer.get_mut(connection_id);

        self.entities.try_for_each_entity(|entity|
        {
            let info = self.entities.info(entity);
            let message = Message::EntitySet{entity, info};

            messager.send_blocking(message)
        })?;

        Ok((connection_id, messager.clone_messager()))
    }

    fn connection_close(&mut self, host: bool, id: ConnectionId, entities: PlayerEntities)
    {
        let removed = self.connection_handler.write().remove_connection(id);

        self.world.remove_player(&mut self.entities, id);

        if host
        {
            self.world.exit(&mut self.entities);
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
        entities.iter().for_each(|&entity|
        {
            writer.send_message(self.entities.remove_message(entity));
        });
    }

    fn process_message_inner(
        &mut self,
        message: Message,
        id: ConnectionId,
        entities: PlayerEntities
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
            entities.player,
            message
        )};

        let message = some_or_return!{self.entities.handle_message(message)};

        match message
        {
            Message::PlayerDisconnect{host} => self.connection_close(host, id, entities),
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
