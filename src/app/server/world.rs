use std::{
    path::PathBuf,
    sync::Arc,
    collections::{hash_map::Entry, HashMap}
};

use parking_lot::{Mutex, RwLock};

use yanyaengine::TransformContainer;

use crate::{
	server::{game_server::ServerEntitiesContainer, ConnectionsHandler},
	common::{
        self,
        EnemyBuilder,
        ObjectsStore,
		TileMap,
        WorldChunkSaver,
        ChunkSaver,
        EntitiesSaver,
        SaveLoad,
		EntityPasser,
        EntityAny,
        EntityType,
        EntityContainer,
		message::Message,
		world::{
            CHUNK_SIZE,
            TILE_SIZE,
			CLIENT_OVERMAP_SIZE,
			CLIENT_OVERMAP_SIZE_Z,
			Chunk,
            ChunkLocal,
			GlobalPos,
			Pos3,
            overmap::OvermapIndexing
		}
	}
};

use world_generator::WorldGenerator;

use server_overmap::ServerOvermap;

pub use world_generator::ParseError;

pub mod world_generator;
mod server_overmap;


pub const SERVER_OVERMAP_SIZE: usize = CLIENT_OVERMAP_SIZE + 1;
pub const SERVER_OVERMAP_SIZE_Z: usize = CLIENT_OVERMAP_SIZE_Z + 1;

type OvermapsType = Arc<RwLock<ObjectsStore<ServerOvermap<WorldChunkSaver>>>>;

#[derive(Debug)]
struct ClientIndexer
{
    size: Pos3<usize>,
    player_position: GlobalPos
}

impl OvermapIndexing for ClientIndexer
{
	fn size(&self) -> Pos3<usize>
	{
		self.size
	}

	fn player_position(&self) -> GlobalPos
	{
		self.player_position
	}
}

#[derive(Debug)]
pub struct World
{
	message_handler: Arc<RwLock<ConnectionsHandler>>,
	world_name: String,
	world_generator: Arc<Mutex<WorldGenerator<WorldChunkSaver>>>,
	chunk_saver: ChunkSaver,
    entities_saver: EntitiesSaver,
	overmaps: OvermapsType,
    client_indexers: ObjectsStore<ClientIndexer>
}

impl World
{
	pub fn new(
		message_handler: Arc<RwLock<ConnectionsHandler>>,
		tilemap: TileMap
	) -> Result<Self, ParseError>
	{
		let world_name = "default".to_owned();

        let world_path = Self::world_path_associated(&world_name);
		let chunk_saver = ChunkSaver::new(world_path.join("chunks"), 100);
		let entities_saver = EntitiesSaver::new(world_path.join("entities"), 10);

		let world_generator = {
			let chunk_saver = WorldChunkSaver::new(world_path.join("world_chunks"), 100);

			WorldGenerator::new(chunk_saver, tilemap, "world_generation/")
		}?;

		let world_generator = Arc::new(Mutex::new(world_generator));

		let overmaps = Arc::new(RwLock::new(ObjectsStore::new()));
        let client_indexers = ObjectsStore::new();

		Ok(Self{
			message_handler,
			world_name,
			world_generator,
			chunk_saver,
            entities_saver,
			overmaps,
            client_indexers
		})
	}

	pub fn add_player(&mut self, position: Pos3<f32>) -> usize
	{
		let size = Pos3::new(SERVER_OVERMAP_SIZE, SERVER_OVERMAP_SIZE, SERVER_OVERMAP_SIZE_Z);
		let overmap = ServerOvermap::new(
			self.world_generator.clone(),
			size,
			position
		);

        let indexer_size = common::world::World::overmap_size();
        let indexer = ClientIndexer{size: indexer_size, player_position: position.rounded()};

        self.client_indexers.push(indexer);

		self.overmaps.write().push(overmap)
	}

	pub fn remove_player(&mut self, id: usize)
	{
		self.client_indexers.remove(id);
		self.overmaps.write().remove(id);
	}

    pub fn player_moved(
        &mut self,
        container: &mut ServerEntitiesContainer,
        id: usize,
        new_position: Pos3<f32>
    )
    {
        let previous_position = &mut self.client_indexers[id].player_position;
        let new_position = new_position.rounded();

        let position_changed = *previous_position != new_position;

        *previous_position = new_position;

        if position_changed
        {
            let mut writer = self.message_handler.write();
            Self::unload_entities(&mut self.entities_saver, container, &mut writer, |global|
            {
                self.client_indexers.iter().any(|(_, indexer)|
                {
                    indexer.inbounds(global)
                })
            });
        }
    }

	pub fn send_chunk(
        &mut self,
        container: &mut ServerEntitiesContainer,
        id: usize,
        pos: GlobalPos
    )
	{
		let chunk = self.load_chunk(container, id, pos);

        let message = Message::ChunkSync{pos, chunk};

		self.message_handler.write().send_single(id, message);
	}

    fn create_entities(
        &self,
        container: &mut ServerEntitiesContainer,
        entities: Vec<EntityAny>
    )
    {
        let mut writer = self.message_handler.write();

        entities.into_iter().for_each(|entity|
        {
            let message = container.push_entity(entity);

            writer.send_message(message);
        });
    }

    fn add_entities(
        &self,
        container: &mut ServerEntitiesContainer,
        chunk_pos: Pos3<f32>,
        chunk: &Chunk
    )
    {
        let spawns = fastrand::usize(0..3);

        let entities: Vec<_> = (0..spawns)
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
            .filter_map(|pos|
            {
                let above = ChunkLocal::from(*pos.pos() + Pos3{x: 0, y: 0, z: 1});
                let has_space = chunk[above].is_none();

                has_space.then(||
                {
                    let pos = chunk_pos + above.pos().map(|x| x as f32 * TILE_SIZE);

                    EntityAny::Enemy(EnemyBuilder::new(pos.into()).build())
                })
            })
            .collect();

        self.create_entities(container, entities);
    }

	fn load_chunk(
        &mut self,
        container: &mut ServerEntitiesContainer,
        id: usize,
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
            if containing_amount == 1
            {
                if let Some(entities) = self.entities_saver.load(pos)
                {
                    self.create_entities(container, entities);
                }
            }
        }

		loaded_chunk.unwrap_or_else(||
		{
			let chunk = self.overmaps.write()[id].generate_chunk(pos);

            self.add_entities(container, pos.into(), &chunk);
                
			self.chunk_saver.save(pos, chunk.clone());

			chunk
		})
	}

    fn collect_to_delete<I>(iter: I) -> (Vec<EntityType>, HashMap<GlobalPos, Vec<EntityAny>>)
    where
        I: Iterator<Item=(EntityType, (GlobalPos, EntityAny))>
    {
        let mut delete_ids = Vec::new();
        let mut delete_entities: HashMap<GlobalPos, Vec<EntityAny>> = HashMap::new();

        for (id, (pos, entity)) in iter
        {
            delete_ids.push(id);

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

        (delete_ids, delete_entities)
    }

    pub fn unload_entities<F>(
        saver: &mut EntitiesSaver,
        container: &mut ServerEntitiesContainer,
        message_handler: &mut ConnectionsHandler,
        keep: F
    )
    where
        F: Fn(GlobalPos) -> bool
    {
        let delete_entities = container.entities_iter()
            .filter(|(_, x)| !x.is_player())
            .filter_map(|(id, x)|
            {
                let pos: Pos3<f32> = (*x.entity_ref().position()).into();
                let pos = pos.rounded();

                (!keep(pos)).then_some((id, (pos, x)))
            });

        let (delete_ids, delete_entities) = Self::collect_to_delete(delete_entities);

        delete_entities.into_iter().for_each(|(pos, entities)|
        {
            saver.save(pos, entities);
        });

        delete_ids.into_iter().for_each(|id|
        {
            let message = container.remove_entity(id);

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
        container: &mut ServerEntitiesContainer,
        id: usize,
        message: Message
    ) -> Option<Message>
	{
        let new_position = (message.entity_type() == Some(EntityType::Player(id))).then(||
        {
            match &message
            {
                Message::EntitySet{entity, ..} =>
                {
                    Some(entity.entity_ref().position())
                },
                Message::EntitySyncTransform{transform, ..} =>
                {
                    Some(&transform.position)
                },
                _ => None
            }
        }).flatten();

        if let Some(new_position) = new_position
        {
            self.player_moved(container, id, (*new_position).into());
        }

		match message
		{
			Message::ChunkRequest{pos} =>
			{
				self.send_chunk(container, id, pos);
				None
			},
			_ => Some(message)
		}
	}
}
