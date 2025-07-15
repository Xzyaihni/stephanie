use std::{
    path::PathBuf,
    sync::Arc,
    rc::Rc,
    cell::RefCell,
    collections::{hash_map::Entry, HashMap}
};

use parking_lot::RwLock;

use crate::{
    debug_config::*,
    server::{DataInfos, ConnectionsHandler},
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

struct EntitiesTracker
{
    pub indexer: ClientIndexer,
    values: ChunksContainer<bool>,
    needs_loading: bool
}

impl EntitiesTracker
{
    fn new(indexer: ClientIndexer) -> Self
    {
        let values = ChunksContainer::new(indexer.size());

        Self{indexer, values, needs_loading: true}
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

    fn mark_ungenerated(&mut self, _pos: LocalPos) {}

    fn generate_missing(&mut self, _offset: Option<Pos3<i32>>)
    {
        self.needs_loading = true;
    }
}

pub struct World
{
    message_handler: Arc<RwLock<ConnectionsHandler>>,
    world_name: String,
    world_generator: Rc<RefCell<WorldGenerator<WorldChunkSaver>>>,
    chunk_saver: ChunkSaver,
    entities_saver: EntitiesSaver,
    enemies_info: Arc<EnemiesInfo>,
    furnitures_info: Arc<FurnituresInfo>,
    loot: Loot,
    overmaps: OvermapsType,
    client_indexers: HashMap<ConnectionId, EntitiesTracker>
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
            client_indexers
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

        Self::unload_entities_inner(&mut self.entities_saver, container, &mut writer, |global|
        {
            self.client_indexers.iter().any(|(_, indexer)|
            {
                indexer.indexer.inbounds(global)
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
        let chunk = self.load_chunk(container, id, pos);

        self.message_handler.write().send_single(id, Message::ChunkSync{pos, chunk});
    }

    fn create_entities_full(
        writer: &mut ConnectionsHandler,
        container: &mut ServerEntities,
        entities: impl Iterator<Item=FullEntityInfo>
    )
    {
        entities.for_each(|entity_info|
        {
            let mut sync_entity = |entity|
            {
                let message = Message::EntitySet{entity, info: Box::new(container.info(entity))};

                writer.send_message(message);

                entity
            };

            let mut create = |info|
            {
                let entity = container.push(false, info);

                sync_entity(entity)
            };

            let entity = entity_info.create(&mut create);

            sync_entity(entity);
        });
    }

    fn update(
        &mut self,
        container: &mut ServerEntities
    )
    {
        self.client_indexers.iter_mut().for_each(|(_, indexer)|
        {
            if indexer.needs_loading
            {
                indexer.values.iter_mut().filter(|(_, x)| !**x).for_each(|(local_pos, x)|
                {
                    *x = true;

                    let pos = indexer.indexer.to_global(local_pos);
                    if let Some(entities) = self.entities_saver.load(pos)
                    {
                        self.entities_saver.save(pos, Vec::new());

                        let mut writer = self.message_handler.write();
                        Self::create_entities_full(&mut writer, container, entities.into_iter());
                    }
                });
            }
        });
    }

    fn load_chunk(
        &mut self,
        container: &mut ServerEntities,
        id: ConnectionId,
        pos: GlobalPos
    ) -> Chunk
    {
        self.chunk_saver.load(pos).unwrap_or_else(||
        {
            let chunk_pos = pos.into();
            let chunk = self.overmaps.borrow_mut().get_mut(&id)
                .expect("id must be valid")
                .generate_chunk(pos, |marker|
                {
                    if DebugConfig::is_enabled(DebugTool::NoSpawns)
                    {
                        return;
                    }

                    let create_infos = marker_tile::CreateInfos{
                        enemies: &self.enemies_info,
                        furnitures: &self.furnitures_info
                    };

                    let mut writer = self.message_handler.write();
                    marker.create(&mut writer, container, create_infos, &self.loot, chunk_pos);
                });

            self.client_indexers.iter_mut().for_each(|(_, indexer)|
            {
                if let Some(pos) = indexer.indexer.to_local(pos)
                {
                    indexer.values[pos] = true;
                }
            });

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
            .filter_map(|(entity, pos)|
            {
                EntityInfo::to_full(container, entity).map(|full_info| (entity, full_info, pos))
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
                Message::SyncPosition{position, ..}
                | Message::SyncPositionRotation{position, ..} =>
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
            self.update(container);
        }

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
