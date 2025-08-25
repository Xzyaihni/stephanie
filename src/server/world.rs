use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    rc::Rc,
    cell::RefCell,
    collections::{hash_map::Entry, HashMap}
};

use parking_lot::RwLock;

use crate::{
    server::{DataInfos, ConnectionsHandler, game_server::{load_world_file, LoadWorldFileError}},
    common::{
        self,
        Loot,
        TileMap,
        WorldChunkSaver,
        ChunkSaver,
        EntitiesSaver,
        EnemiesInfo,
        FurnituresInfo,
        SaveLoad,
        AnyEntities,
        EntityPasser,
        Entity,
        EntityInfo,
        FullEntityInfo,
        ConnectionId,
        ChunksContainer,
        chunk_saver::with_temp_save,
        entity::ServerEntities,
        message::Message,
        world::{
            CLIENT_OVERMAP_SIZE,
            CLIENT_OVERMAP_SIZE_Z,
            TilePos,
            Tile,
            Chunk,
            LocalPos,
            GlobalPos,
            Pos3,
            overmap::{Overmap, OvermapIndexing, CommonIndexing}
        }
    }
};

use world_generator::WorldGenerator;

use server_overmap::ServerOvermap;

pub use world_generator::ParseError;
pub use marker_tile::{MarkerTile, MarkerKind};

pub mod world_generator;
mod server_overmap;

mod marker_tile;


pub const SERVER_OVERMAP_SIZE: usize = CLIENT_OVERMAP_SIZE + 3;
pub const SERVER_OVERMAP_SIZE_Z: usize = CLIENT_OVERMAP_SIZE_Z + 3;

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

struct EntitiesTracker
{
    pub indexer: ClientIndexer,
    values: ChunksContainer<bool>
}

impl EntitiesTracker
{
    fn new(indexer: ClientIndexer) -> Self
    {
        let values = ChunksContainer::new(indexer.size());

        Self{indexer, values}
    }
}

impl CommonIndexing for EntitiesTracker
{
    fn size(&self) -> Pos3<usize>
    {
        self.indexer.size()
    }
}

impl OvermapIndexing for EntitiesTracker
{
    fn player_position(&self) -> GlobalPos
    {
        self.indexer.player_position()
    }
}

impl Overmap<bool> for EntitiesTracker
{
    fn remove(&mut self, pos: LocalPos)
    {
        self.values[pos] = false;
    }

    fn swap(&mut self, a: LocalPos, b: LocalPos)
    {
        self.values.swap(a, b);
    }

    fn get_local(&self, pos: LocalPos) -> &bool
    {
        &self.values[pos]
    }

    fn generate_missing(&mut self, _offset: Option<Pos3<i32>>) {}
}

pub struct World
{
    message_handler: Arc<RwLock<ConnectionsHandler>>,
    world_name: String,
    world_generator: Rc<RefCell<WorldGenerator<WorldChunkSaver>>>,
    chunk_saver: ChunkSaver,
    pub entities_saver: EntitiesSaver,
    enemies_info: Arc<EnemiesInfo>,
    furnitures_info: Arc<FurnituresInfo>,
    loot: Loot,
    overmaps: OvermapsType,
    client_indexers: HashMap<ConnectionId, EntitiesTracker>,
    pub time: f64
}

impl World
{
    pub fn new(
        message_handler: Arc<RwLock<ConnectionsHandler>>,
        tilemap: Rc<TileMap>,
        data_infos: DataInfos,
        world_name: String
    ) -> Result<Self, ParseError>
    {
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

        let loot = Loot::new(data_infos.items_info, "items/loot.scm")?;

        let time = match load_world_file(&Self::world_save_path_associated(&world_name))
        {
            Ok(x) => x.unwrap_or(0.0),
            Err(err) =>
            {
                match err
                {
                    LoadWorldFileError::Io(err) =>
                    {
                        eprintln!("error trying to open world save file: {err}");
                    },
                    LoadWorldFileError::Load(err) =>
                    {
                        eprintln!("error trying to load world: {err}");
                    }
                }

                0.0
            }
        };

        Ok(Self{
            message_handler,
            world_name,
            world_generator,
            chunk_saver,
            entities_saver,
            enemies_info: data_infos.enemies_info,
            furnitures_info: data_infos.furnitures_info,
            loot,
            overmaps,
            client_indexers,
            time
        })
    }

    fn set_tile_local(&mut self, pos: TilePos, tile: Tile)
    {
        self.modify_chunk(pos, |chunk|
        {
            *chunk = chunk.with_set_tile(pos.local, tile);
        });
    }

    pub fn modify_chunk(&mut self, pos: TilePos, f: impl FnOnce(&mut Chunk))
    {
        if let Some(mut chunk) = self.chunk_saver.load(pos.chunk)
        {
            f(&mut chunk);

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

        self.client_indexers.insert(id, EntitiesTracker::new(indexer));
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
            let previous_position = &mut indexer.indexer.player_position;

            let new_position = new_position.rounded();

            let offset = new_position - *previous_position;

            if offset.0 != Pos3::repeat(0)
            {
                *previous_position = new_position;

                // this will unload entities even far outside the chunks if somehow any manage to leave
                self.unload_entities(container);

                if let Some(indexer) = self.client_indexers.get_mut(&id)
                {
                    indexer.position_offset(offset.0);

                    let mut overmaps = self.overmaps.borrow_mut();
                    if let Some(overmap) = overmaps.get_mut(&id)
                    {
                        overmap.move_to(new_position);
                    }
                }
            }
        }
    }

    pub fn unload_entities(
        &mut self,
        container: &mut ServerEntities
    )
    {
        let mut writer = self.message_handler.write();

        Self::unload_entities_inner(container, &mut writer, |global|
        {
            self.client_indexers.iter().any(|(_, indexer)|
            {
                indexer.indexer.inbounds(global)
            })
        });
    }

    pub fn exit(&mut self, container: &mut ServerEntities)
    {
        if let Err(err) = fs::create_dir_all(self.world_path())
        {
            eprintln!("error trying to create world directory: {err}");
        }

        if let Err(err) = with_temp_save(self.world_save_path(), self.time)
        {
            eprintln!("error trying to save world info: {err}");
        }

        let mut writer = self.message_handler.write();
        Self::unload_entities_inner(container, &mut writer, |_global|
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
        let indexer = self.client_indexers[&id].indexer.clone();

        let ordering = indexer.default_ordering();

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
        let message = self.load_chunk(container, id, pos);

        self.message_handler.write().send_single(id, message);
    }

    fn create_entities_full(
        container: &mut ServerEntities,
        entities: impl Iterator<Item=FullEntityInfo>
    ) -> Vec<(Entity, EntityInfo)>
    {
        let mut output = Vec::new();
        entities.for_each(|entity_info|
        {
            let mut create = |info: EntityInfo| -> Entity
            {
                let entity = container.push_eager(false, info.clone());

                output.push((entity, info));

                entity
            };

            entity_info.create(&mut create);
        });

        output
    }

    fn load_chunk(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId,
        pos: GlobalPos
    ) -> Message
    {
        let entities = self.entities_saver.load(pos).inspect(|_|
        {
            self.entities_saver.save(pos, Vec::new());
        }).unwrap_or_default();

        let mut entities = Self::create_entities_full(container, entities.into_iter());

        let chunk = self.chunk_saver.load(pos).unwrap_or_else(||
        {
            let mut chunk_entities = Vec::new();

            let chunk_pos = pos.into();
            let chunk = self.overmaps.borrow_mut().get_mut(&id)
                .expect("id must be valid")
                .generate_chunk(pos, |marker|
                {
                    let create_infos = marker_tile::CreateInfos{
                        enemies: &self.enemies_info,
                        furnitures: &self.furnitures_info
                    };

                    if let Some(info) = marker.create(create_infos, &self.loot, chunk_pos)
                    {
                        let entity = container.push_eager(false, info.clone());
                        chunk_entities.push((entity, info));
                    }
                });

            entities.extend(chunk_entities);

            self.client_indexers.iter_mut().for_each(|(_, indexer)|
            {
                if let Some(pos) = indexer.indexer.to_local(pos)
                {
                    indexer.values[pos] = true;
                }
            });

            self.chunk_saver.save(pos, chunk.clone());

            chunk
        });

        Message::ChunkSync{pos, chunk, entities}
    }

    pub fn collect_to_delete<T, I>(iter: I) -> HashMap<GlobalPos, Vec<T>>
    where
        I: Iterator<Item=(T, GlobalPos)>
    {
        let mut delete_entities: HashMap<GlobalPos, Vec<T>> = HashMap::new();

        for (entity, pos) in iter
        {
            match delete_entities.entry(pos)
            {
                Entry::Occupied(mut occupied) =>
                {
                    occupied.get_mut().push(entity);
                },
                Entry::Vacant(vacant) =>
                {
                    vacant.insert(vec![entity]);
                }
            }
        }

        delete_entities
    }

    fn unload_entities_inner<F>(
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
            .filter(|(entity, _pos)|
            {
                // if a player is somehow too far away from their own overmap dont unload them by accident
                !container.player_exists(*entity)
            });

        Self::collect_to_delete::<Entity, _>(delete_entities).into_iter().for_each(|(pos, delete_ids)|
        {
            message_handler.send_message(Message::EntityRemoveChunk{
                pos,
                entities: container.send_remove_many::<true>(delete_ids)
            });
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

    fn world_save_path(&self) -> PathBuf
    {
        Self::world_save_path_associated(&self.world_name)
    }

    fn world_save_path_associated(name: &str) -> PathBuf
    {
        Self::world_path_associated(name).join("world.save")
    }

    pub fn sync_camera(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId,
        position: Pos3<f32>
    )
    {
        self.player_moved(container, id, position);
    }

    pub fn handle_message(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId,
        message: Message
    ) -> Option<Message>
    {
        #[cfg(debug_assertions)]
        {
            use crate::common::message::DebugMessage;

            match message
            {
                Message::DebugMessage(DebugMessage::PrintServerOvermaps) =>
                {
                    eprintln!("server overmaps: {:#?}", self.overmaps.borrow());
                    return None;
                },
                _ => ()
            }
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
            Message::SyncCamera{position} =>
            {
                self.sync_camera(container, id, position);
                None
            },
            Message::SyncWorldTime{time} =>
            {
                self.time = time;
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
        common::{
            AnyEntities,
            MessagePasser,
            BufferSender,
            FurnituresInfo,
            ItemsInfo,
            EnemiesInfo,
            CharactersInfo,
            CharacterId,
            message::MessageBuffer
        }
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

            let player = passer.write().connect(PlayerInfo{
                message_buffer: MessageBuffer::new(),
                message_passer: MessagePasser::new(stream),
                entity: Some(entities.push_eager(false, EntityInfo{..Default::default()})),
                name: "test_player".to_owned(),
                host: true
            });

            let tilemap = TileMap::parse("tiles/tiles.json", "textures/tiles/")
                .unwrap();

            let mut world = World::new(
                passer.clone(),
                Rc::new(tilemap.tilemap),
                DataInfos{
                    items_info: Arc::new(ItemsInfo::empty()),
                    enemies_info: Arc::new(EnemiesInfo::empty()),
                    furnitures_info: Arc::new(FurnituresInfo::empty()),
                    characters_info: Arc::new(CharactersInfo::new()),
                    player_character: CharacterId::from(0)
                },
                "default".to_owned()
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
                    Message::ChunkSync{pos, chunk, entities: _} =>
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
                    Message::ChunkSync{pos, chunk, entities: _} =>
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
