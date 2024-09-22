use std::{
    path::PathBuf,
    sync::Arc,
    rc::Rc,
    cell::RefCell,
    collections::{hash_map::Entry, HashMap}
};

use parking_lot::RwLock;

use nalgebra::Vector3;

use crate::{
    server::ConnectionsHandler,
    common::{
        self,
        SpecialTile,
        FurnitureBuilder,
        EnemyBuilder,
        TileMap,
        WorldChunkSaver,
        ChunkSaver,
        ItemsInfo,
        EntitiesSaver,
        EnemiesInfo,
        SaveLoad,
        AnyEntities,
        EntityPasser,
        Entity,
        EntityInfo,
        FullEntityInfo,
        ConnectionId,
        entity::ServerEntities,
        message::Message,
        world::{
            CHUNK_SIZE,
            TILE_SIZE,
            CLIENT_OVERMAP_SIZE,
            CLIENT_OVERMAP_SIZE_Z,
            TilePos,
            Tile,
            Chunk,
            ChunkLocal,
            GlobalPos,
            Pos3,
            overmap::{Overmap, OvermapIndexing, CommonIndexing}
        }
    }
};

use world_generator::WorldGenerator;

use server_overmap::ServerOvermap;

pub use world_generator::ParseError;

pub mod world_generator;
mod server_overmap;

mod spawner;


pub const SERVER_OVERMAP_SIZE: usize = CLIENT_OVERMAP_SIZE + 1;
pub const SERVER_OVERMAP_SIZE_Z: usize = CLIENT_OVERMAP_SIZE_Z + 1;

type OvermapsType = Rc<RefCell<HashMap<ConnectionId, ServerOvermap<WorldChunkSaver>>>>;

#[derive(Debug, Clone)]
struct ClientIndexer
{
    size: Pos3<usize>,
    player_position: GlobalPos
}

impl CommonIndexing for ClientIndexer
{
    fn size(&self) -> Pos3<usize>
    {
        self.size
    }
}

impl OvermapIndexing for ClientIndexer
{
    fn player_position(&self) -> GlobalPos
    {
        self.player_position
    }
}

pub struct World
{
    message_handler: Arc<RwLock<ConnectionsHandler>>,
    tilemap: Rc<TileMap>,
    world_name: String,
    world_generator: Rc<RefCell<WorldGenerator<WorldChunkSaver>>>,
    chunk_saver: ChunkSaver,
    entities_saver: EntitiesSaver,
    enemies_info: Arc<EnemiesInfo>,
    items_info: Arc<ItemsInfo>,
    overmaps: OvermapsType,
    client_indexers: HashMap<ConnectionId, ClientIndexer>
}

impl World
{
    pub fn new(
        message_handler: Arc<RwLock<ConnectionsHandler>>,
        tilemap: TileMap,
        enemies_info: Arc<EnemiesInfo>,
        items_info: Arc<ItemsInfo>
    ) -> Result<Self, ParseError>
    {
        let tilemap = Rc::new(tilemap);

        let world_name = "default".to_owned();

        let world_path = Self::world_path_associated(&world_name);
        let chunk_saver = ChunkSaver::new(world_path.join("chunks"), 100);
        let entities_saver = EntitiesSaver::new(world_path.join("entities"), 0);

        let world_generator = {
            let chunk_saver = WorldChunkSaver::new(world_path.join("world_chunks"), 100);

            WorldGenerator::new(chunk_saver, tilemap.clone(), "world_generation/")
        }?;

        let world_generator = Rc::new(RefCell::new(world_generator));

        let overmaps = Rc::new(RefCell::new(HashMap::new()));
        let client_indexers = HashMap::new();

        Ok(Self{
            message_handler,
            tilemap,
            world_name,
            world_generator,
            chunk_saver,
            entities_saver,
            enemies_info,
            items_info,
            overmaps,
            client_indexers
        })
    }

    fn set_tile_local(&mut self, pos: TilePos, tile: Tile)
    {
        if let Some(chunk) = self.chunk_saver.load(pos.chunk)
        {
            let chunk = chunk.with_set_tile(pos.local, tile);

            self.chunk_saver.save(pos.chunk, chunk);
        }
    }

    pub fn add_player(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId,
        position: Pos3<f32>
    )
    {
        let size = Pos3::new(SERVER_OVERMAP_SIZE, SERVER_OVERMAP_SIZE, SERVER_OVERMAP_SIZE_Z);
        let overmap = ServerOvermap::new(
            self.world_generator.clone(),
            size,
            position
        );

        let indexer_size = common::world::World::overmap_size();
        let indexer = ClientIndexer{size: indexer_size, player_position: position.rounded()};

        self.client_indexers.insert(id, indexer);
        self.overmaps.borrow_mut().insert(id, overmap);

        self.unload_entities(container);
    }

    pub fn remove_player(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId
    )
    {
        self.client_indexers.remove(&id);
        self.overmaps.borrow_mut().remove(&id);

        self.unload_entities(container);
    }

    pub fn player_moved(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId,
        new_position: Pos3<f32>
    )
    {
        if let Some(indexer) = self.client_indexers.get_mut(&id)
        {
            let previous_position = &mut indexer.player_position;

            let new_position = new_position.rounded();

            let position_changed = *previous_position != new_position;

            *previous_position = new_position;

            if position_changed
            {
                self.unload_entities(container);
            }
        }
    }

    pub fn unload_entities(
        &mut self,
        container: &mut ServerEntities
    )
    {
        let mut writer = self.message_handler.write();
        let overmaps = self.overmaps.borrow();

        Self::unload_entities_inner(&mut self.entities_saver, container, &mut writer, |global|
        {
            self.client_indexers.iter().zip(overmaps.values()).any(|((_, indexer), overmap)|
            {
                indexer.inbounds(global)
                    || overmap.contains(global)
            })
        });
    }

    pub fn exit(&mut self, container: &mut ServerEntities)
    {
        let mut writer = self.message_handler.write();
        Self::unload_entities_inner(&mut self.entities_saver, container, &mut writer, |_global|
        {
            false
        });
    }

    pub fn send_all(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId
    )
    {
        let indexer = self.client_indexers[&id].clone();

        let ordering = indexer.default_ordering(indexer.clone().positions());

        ordering.iter().for_each(|pos|
        {
            self.send_chunk(container, id, indexer.to_global(*pos));
        });
    }

    pub fn send_chunk(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId,
        pos: GlobalPos
    )
    {
        let chunk = self.load_chunk(container, id, pos);

        self.message_handler.write().send_single(id, Message::ChunkSync{pos, chunk});
    }

    fn create_entities_full(
        &self,
        container: &mut ServerEntities,
        entities: impl Iterator<Item=FullEntityInfo>
    )
    {
        let mut writer = self.message_handler.write();

        entities.for_each(|entity_info|
        {
            let mut create = |info|
            {
                let entity = container.push(false, info);
                let message = Message::EntitySet{entity, info: container.info(entity)};

                writer.send_message(message);

                entity
            };

            let info = entity_info.create(&mut create);

            create(info);
        });
    }

    fn create_entities(
        &self,
        container: &mut ServerEntities,
        entities: impl Iterator<Item=EntityInfo>
    )
    {
        let mut writer = self.message_handler.write();

        entities.for_each(|entity_info|
        {
            let message = container.push_message(entity_info);

            writer.send_message(message);
        });
    }

    fn add_on_ground<'a>(
        chunk_pos: Pos3<f32>,
        chunk: &'a Chunk,
        amount: usize,
        f: impl Fn(Vector3<f32>) -> Option<EntityInfo> + 'a
    ) -> impl Iterator<Item=EntityInfo> + 'a
    {
        (0..amount)
            .map(|_|
            {
                ChunkLocal::new(
                    fastrand::usize(0..CHUNK_SIZE),
                    fastrand::usize(0..CHUNK_SIZE),
                    fastrand::usize(0..CHUNK_SIZE - 1)
                )
            })
            .filter_map(|pos|
            {
                let mut current_pos = pos;

                let is_ground = |p|
                {
                    !chunk[p].is_none()
                };

                loop
                {
                    if is_ground(current_pos)
                    {
                        return Some(current_pos);
                    }

                    if current_pos.pos().z == 0
                    {
                        return None;
                    }

                    let new_pos = *current_pos.pos();
                    let new_pos = Pos3{z: new_pos.z - 1, ..new_pos};

                    current_pos = ChunkLocal::from(new_pos);
                }
            })
            .filter_map(move |pos|
            {
                let above = ChunkLocal::from(*pos.pos() + Pos3{x: 0, y: 0, z: 1});
                let has_space = chunk[above].is_none();

                has_space.then(||
                {
                    let half_tile = TILE_SIZE / 2.0;
                    let pos = chunk_pos + above.pos().map(|x| x as f32 * TILE_SIZE) + half_tile;

                    f(pos.into())
                }).flatten()
            })
    }

    fn create_spawners(
        &self,
        container: &mut ServerEntities,
        chunk_pos: Pos3<f32>,
        chunk: &mut Chunk
    )
    {
        chunk.iter_mut().for_each(|(pos, tile)|
        {
            let info = self.tilemap.info(*tile);

            if let Some(SpecialTile::Spawner(spawner)) = &info.special
            {
                let pos = chunk_pos + pos.pos().map(|x| x as f32 * TILE_SIZE);

                spawner::create_spawner(container, pos, spawner);

                *tile = Tile::none();
            }
        });
    }

    fn add_entities(
        &self,
        container: &mut ServerEntities,
        chunk_pos: Pos3<f32>,
        chunk: &mut Chunk
    )
    {
        self.create_spawners(container, chunk_pos, chunk);

        let spawns = 0; let remove_me = ();// fastrand::usize(0..3);
        let crates = fastrand::usize(0..2);

        let entities = Self::add_on_ground(chunk_pos, chunk, spawns, |pos|
        {
            let picked = self.enemies_info.weighted_random(1.0)?;

            Some(EnemyBuilder::new(
                &self.enemies_info,
                &self.items_info,
                picked,
                pos
            ).build())
        }).chain(Self::add_on_ground(chunk_pos, chunk, crates, |pos|
        {
            Some(FurnitureBuilder::new(&self.items_info, pos).build())
        })).map(|mut entity_info|
        {
            if entity_info.saveable.is_none()
            {
                entity_info.saveable = Some(());
            }

            entity_info
        });

        self.create_entities(container, entities);
    }

    fn load_chunk(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId,
        pos: GlobalPos
    ) -> Chunk
    {
        let loaded_chunk = self.chunk_saver.load(pos);

        if loaded_chunk.is_some()
        {
            let containing_amount = self.client_indexers.iter().filter(|(_, indexer)|
            {
                indexer.inbounds(pos)
            }).count();

            // only 1 overmap contains chunk
            if containing_amount <= 1
            {
                if let Some(entities) = self.entities_saver.load(pos)
                {
                    self.entities_saver.save(pos, Vec::new());
                    self.create_entities_full(container, entities.into_iter());
                }
            }
        }

        loaded_chunk.unwrap_or_else(||
        {
            let mut chunk = self.overmaps.borrow_mut().get_mut(&id)
                .expect("id must be valid")
                .generate_chunk(pos);

            self.add_entities(container, pos.into(), &mut chunk);
                
            self.chunk_saver.save(pos, chunk.clone());

            chunk
        })
    }

    fn collect_to_delete<I>(iter: I) -> (Vec<Entity>, HashMap<GlobalPos, Vec<FullEntityInfo>>)
    where
        I: Iterator<Item=(Entity, FullEntityInfo, GlobalPos)>
    {
        let mut delete_ids = Vec::new();
        let mut delete_entities: HashMap<GlobalPos, Vec<FullEntityInfo>> = HashMap::new();

        for (entity, info, pos) in iter
        {
            delete_ids.push(entity);

            match delete_entities.entry(pos)
            {
                Entry::Occupied(mut occupied) =>
                {
                    occupied.get_mut().push(info);
                },
                Entry::Vacant(vacant) =>
                {
                    vacant.insert(vec![info]);
                }
            }
        }

        (delete_ids, delete_entities)
    }

    fn unload_entities_inner<F>(
        saver: &mut EntitiesSaver,
        container: &mut ServerEntities,
        message_handler: &mut ConnectionsHandler,
        keep: F
    )
    where
        F: Fn(GlobalPos) -> bool
    {
        let delete_entities = container.saveable.iter()
            .rev()
            .map(|(_, x)| x.entity)
            .filter_map(|entity|
            {
                container.transform(entity).map(|transform|
                {
                    let pos: Pos3<f32> = transform.position.into();

                    (entity, pos.rounded())
                })
            })
            .filter(|(_entity, pos)|
            {
                !keep(*pos)
            })
            .map(|(entity, pos)|
            {
                let info = container.info(entity);

                (entity, info.to_full(container), pos)
            });

        let (delete_ids, delete_entities) = Self::collect_to_delete(delete_entities);

        delete_entities.into_iter().for_each(|(pos, mut entities)|
        {
            if let Some(mut previous) = saver.load(pos)
            {
                previous.append(&mut entities);

                entities = previous;
            }

            saver.save(pos, entities);
        });

        delete_ids.into_iter().for_each(|entity|
        {
            let message = container.remove_message(entity);

            message_handler.send_message(message);
        });
    }

    #[allow(dead_code)]
    fn world_path(&self) -> PathBuf
    {
        Self::world_path_associated(&self.world_name)
    }

    fn world_path_associated(name: &str) -> PathBuf
    {
        PathBuf::from("worlds").join(name)
    }

    pub fn handle_message(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId,
        entity: Entity,
        message: Message
    ) -> Option<Message>
    {
        let new_position = (message.entity() == Some(entity)).then(||
            match &message
            {
                Message::EntitySet{info, ..} =>
                {
                    info.transform.as_ref().map(|x| x.position)
                },
                Message::SetTarget{target, ..} =>
                {
                    Some(target.position)
                },
                Message::SetTargetPosition{position, ..}
                | Message::SyncPosition{position, ..} =>
                {
                    Some(*position)
                }
                Message::SetTransform{component, ..} =>
                {
                    Some(component.position)
                },
                _ => None
            }
        ).flatten();

        if let Some(new_position) = new_position
        {
            self.player_moved(container, id, new_position.into());
        }

        match message
        {
            Message::SetTile{pos, tile} =>
            {
                self.set_tile_local(pos, tile);
                None
            },
            Message::ChunkRequest{pos} =>
            {
                self.send_chunk(container, id, pos);
                None
            },
            _ => Some(message)
        }
    }
}

#[cfg(test)]
mod tests
{
    use std::{fs, thread, net::{TcpStream, TcpListener}};

    use crate::{
        server::connections_handler::PlayerInfo,
        common::{AnyEntities, MessagePasser, BufferSender, message::MessageBuffer}
    };

    use super::*;


    #[ignore]
    #[test]
    fn world_full()
    {
        if PathBuf::from("worlds").exists()
        {
            fs::remove_dir_all("worlds").unwrap();
        }

        fn do_with_world(
            stream: TcpStream,
            f: impl FnOnce(&mut ServerEntities, &mut World, ConnectionId)
        )
        {
            let passer = Arc::new(RwLock::new(ConnectionsHandler::new(3)));

            let mut entities = ServerEntities::new(None);

            let player = passer.write().connect(PlayerInfo::new(
                MessageBuffer::new(),
                MessagePasser::new(stream),
                entities.push_eager(false, EntityInfo{..Default::default()}),
                "test_player".to_owned()
            ));

            let tilemap = TileMap::parse("tiles/tiles.json", "textures/tiles/")
                .unwrap();

            let mut world = World::new(
                passer.clone(),
                tilemap.tilemap,
                Arc::new(EnemiesInfo::empty()),
                Arc::new(ItemsInfo::empty())
            ).unwrap();

            world.add_player(&mut entities, player, Pos3::new(0.0, 0.0, 0.0));

            f(&mut entities, &mut world, player);

            passer.write().send_buffered().unwrap();

            world.remove_player(&mut entities, player);

            passer.write().remove_connection(player).unwrap();
            passer.write().send_buffered().unwrap();
        }

        let listener = TcpListener::bind("localhost:12345").unwrap();

        let handle0 = thread::spawn(move ||
        {
            let stream = listener.accept().unwrap().0;
            do_with_world(stream, |entities, world, player|
            {
                world.send_all(entities, player);
            });
        });

        let listener = TcpListener::bind("localhost:12346").unwrap();

        let handle1 = thread::spawn(move ||
        {
            let stream = listener.accept().unwrap().0;
            do_with_world(stream, |entities, world, player|
            {
                world.send_all(entities, player);
            });
        });

        let receiver = |address|
        {
            let receiver = TcpStream::connect(address).unwrap();

            MessagePasser::new(receiver)
        };

        let mut remembered = HashMap::new();

        {
            let mut receiver = receiver("localhost:12345");

            for message in receiver.receive().unwrap()
            {
                match message
                {
                    Message::ChunkSync{pos, chunk} =>
                    {
                        remembered.insert(pos, chunk);
                    },
                    _ => ()
                }
            }
        }

        handle0.join().unwrap();

        let mut deferred_panic = None;

        let mut total = 0;
        let mut incorrect = 0;

        {
            let mut receiver = receiver("localhost:12346");

            for message in receiver.receive().unwrap()
            {
                match message
                {
                    Message::ChunkSync{pos, chunk} =>
                    {
                        total += 1;

                        let value = remembered.get(&pos).unwrap().clone();

                        if value != chunk
                        {
                            incorrect += 1;
                            eprintln!("{pos:?} has a mismatch");

                            deferred_panic = Some(move ||
                            {
                                panic!("at {pos:?}: {value:#?} != {chunk:#?}");
                            });
                        }
                    },
                    _ => ()
                }
            }
        }

        handle1.join().unwrap();

        if let Some(deferred_panic) = deferred_panic
        {
            eprintln!("{incorrect} incorrect out of {total}");
            deferred_panic();
        }
    }
}
