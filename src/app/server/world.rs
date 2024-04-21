use std::{
    path::PathBuf,
    sync::Arc
};

use parking_lot::{Mutex, RwLock};

use crate::{
	server::{game_server::ServerEntitiesContainer, ConnectionsHandler},
	common::{
        EnemyBuilder,
        ObjectsStore,
		TileMap,
        WorldChunkSaver,
        ChunkSaver,
        EntitiesSaver,
        SaveLoad,
		EntityPasser,
        EntityAny,
		message::Message,
		world::{
            CHUNK_SIZE,
            TILE_SIZE,
			CLIENT_OVERMAP_SIZE,
			CLIENT_OVERMAP_SIZE_Z,
			Chunk,
            ChunkLocal,
			GlobalPos,
			Pos3
		}
	}
};

use world_generator::WorldGenerator;

use server_overmap::ServerOvermapData;

pub use world_generator::ParseError;

pub mod world_generator;
mod server_overmap;


pub const SERVER_OVERMAP_SIZE: usize = CLIENT_OVERMAP_SIZE + 1;
pub const SERVER_OVERMAP_SIZE_Z: usize = CLIENT_OVERMAP_SIZE_Z + 1;

type OvermapsType = Arc<RwLock<ObjectsStore<ServerOvermapData<WorldChunkSaver>>>>;

#[derive(Debug)]
pub struct World
{
	message_handler: Arc<RwLock<ConnectionsHandler>>,
	world_name: String,
	world_generator: Arc<Mutex<WorldGenerator<WorldChunkSaver>>>,
	chunk_saver: ChunkSaver,
    entities_saver: EntitiesSaver,
	overmaps: OvermapsType
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

		Ok(Self{
			message_handler,
			world_name,
			world_generator,
			chunk_saver,
            entities_saver,
			overmaps
		})
	}

	pub fn add_player(&mut self, position: Pos3<f32>) -> usize
	{
		let size = Pos3::new(SERVER_OVERMAP_SIZE, SERVER_OVERMAP_SIZE, SERVER_OVERMAP_SIZE_Z);
		let overmap = ServerOvermapData::new(
			self.world_generator.clone(),
			size,
			position
		);

		self.overmaps.write().push(overmap)
	}

	pub fn remove_player(&mut self, id: usize)
	{
		self.overmaps.write().remove(id);
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
        &mut self,
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
        &mut self,
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
            let containing_amount = self.overmaps.read().iter().filter(|(_, overmap)|
            {
                overmap.inbounds_chunk(pos)
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
			let chunk = {
                let overmap = &mut self.overmaps.write()[id];
                let mut overmap = overmap.attach_info(&mut self.entities_saver);

                overmap.generate_chunk(pos)
            };

            self.add_entities(container, pos.into(), &chunk);

			self.chunk_saver.save(pos, chunk.clone());

			chunk
		})
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
