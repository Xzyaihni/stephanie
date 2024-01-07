use std::sync::Arc;

use parking_lot::Mutex;

use super::world_generator::{
    WORLD_CHUNK_SIZE,
    CHUNK_RATIO,
    WorldGenerator,
    WorldChunk
};

use crate::common::{
    SaveLoad,
    world::{
        CHUNK_SIZE,
        LocalPos,
        GlobalPos,
        Pos3,
        Chunk,
        chunk::tile::Tile,
        overmap::{Overmap, OvermapIndexing, ChunksContainer}
    }
};


#[derive(Debug)]
struct Indexer
{
	pub size: Pos3<usize>,
	pub player_position: GlobalPos
}

impl Indexer
{
	pub fn new(size: Pos3<usize>, player_position: GlobalPos) -> Self
	{
		Self{size, player_position}
	}
}

impl OvermapIndexing for Indexer
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
pub struct ServerOvermap<S>
{
	world_generator: Arc<Mutex<WorldGenerator<S>>>,
	world_chunks: ChunksContainer<Option<WorldChunk>>,
	indexer: Indexer
}

impl<S: SaveLoad<WorldChunk>> ServerOvermap<S>
{
	pub fn new(
		world_generator: Arc<Mutex<WorldGenerator<S>>>,
		size: Pos3<usize>,
		player_position: Pos3<f32>
	) -> Self
	{
		assert_eq!(CHUNK_SIZE % WORLD_CHUNK_SIZE.x, 0);
		assert_eq!(CHUNK_SIZE % WORLD_CHUNK_SIZE.y, 0);
		assert_eq!(CHUNK_SIZE % WORLD_CHUNK_SIZE.z, 0);

		let size = CHUNK_RATIO * size;

		let indexer = Indexer::new(size, player_position.rounded());

		let world_chunks = ChunksContainer::new(size);

		let mut this = Self{
			world_generator,
			world_chunks,
			indexer
		};

		this.generate_missing();

		this
	}

	pub fn generate_chunk(&mut self, pos: GlobalPos) -> Chunk
	{
        let chunk_ratio = GlobalPos::from(CHUNK_RATIO);

        let pos = pos * chunk_ratio;

		let shift_offset = self.over_bounds_with_padding(
            pos,
            Pos3::repeat(1),
            chunk_ratio.0 + 1
        );

		let non_shifted = shift_offset.0 == Pos3::repeat(0_i32);

		if !non_shifted
		{
			self.shift_overmap_by(shift_offset);
		}

		self.generate_existing_chunk(self.to_local(pos).unwrap())
	}

	fn shift_overmap_by(&mut self, shift_offset: GlobalPos)
	{
		let new_player_position = self.indexer.player_position + shift_offset;

		self.indexer.player_position = new_player_position;

		self.position_offset(shift_offset);
	}

	fn generate_existing_chunk(&self, local_pos: LocalPos) -> Chunk
	{
        let mut chunk = Chunk::new();

        for z in 0..CHUNK_RATIO.z
        {
            for y in 0..CHUNK_RATIO.y
            {
                for x in 0..CHUNK_RATIO.x
                {
                    let this_pos = Pos3::new(x, y, z);

                    let local_pos = local_pos + this_pos;

		            let group = local_pos.always_group().expect("chunk must not touch edges");
		            let group = group.map(|position|
                    {
                        self.world_chunks[position].unwrap()
                    });

		            let world_chunk = self.world_generator.lock().generate_chunk(group);

                    Self::partially_fill(&mut chunk, world_chunk, this_pos);
                }
            }
        }

        chunk
	}

    fn partially_fill(chunk: &mut Chunk, world_chunk: ChunksContainer<Tile>, pos: Pos3<usize>)
    {
        let size = world_chunk.size();
        for z in 0..size.z
        {
            for y in 0..size.y
            {
                for x in 0..size.x
                {
                    let this_pos = Pos3::new(x, y, z);
                    chunk[pos + this_pos] = world_chunk[this_pos];
                }
            }
        }
    }
}

impl<S: SaveLoad<WorldChunk>> Overmap<WorldChunk> for ServerOvermap<S>
{
	fn remove(&mut self, pos: LocalPos)
	{
        self.world_chunks[pos] = None;
	}

	fn swap(&mut self, a: LocalPos, b: LocalPos)
	{
		self.world_chunks.swap(a, b);
	}

	fn get_local(&self, pos: LocalPos) -> &Option<WorldChunk>
	{
		&self.world_chunks[pos]
	}

	fn mark_ungenerated(&mut self, _pos: LocalPos) {}

	fn generate_missing(&mut self)
	{
		self.world_generator.lock().generate_missing(&mut self.world_chunks, &self.indexer);
	}
}

impl<S> OvermapIndexing for ServerOvermap<S>
{
	fn size(&self) -> Pos3<usize>
	{
		self.indexer.size
	}

	fn player_position(&self) -> GlobalPos
	{
		self.indexer.player_position
	}
}

#[cfg(test)]
mod tests
{
    use super::*;

    use std::collections::HashMap;

    use crate::common::TileMap;


    struct TestSaver<T>
    {
        data: HashMap<GlobalPos, T>
    }

    impl<T> TestSaver<T>
    {
        pub fn new() -> Self
        {
            Self{data: HashMap::new()}
        }
    }

    impl<T: Clone> SaveLoad<T> for TestSaver<T>
    {
        fn save(&mut self, pos: GlobalPos, chunk: T)
        {
            self.data.insert(pos, chunk);
        }

        fn load(&mut self, pos: GlobalPos) -> Option<T>
        {
            self.data.get(&pos).cloned()
        }
    }

    #[test]
    fn moving_around()
    {
        let saver = TestSaver::new();

        let tiles = "tiles/tiles.json";

        let tilemap = TileMap::parse(tiles, "textures/tiles/").unwrap().tilemap;

        let world_generator = Arc::new(Mutex::new(
            WorldGenerator::new(saver, tilemap, "world_generation/city.json").unwrap()
        ));

        let size = Pos3::new(10, 11, 12);

        let random_chunk = ||
        {
            let r = |s: usize|
            {
                let ps = (s as i32).pow(3);

                fastrand::i32(0..(ps * 2)) - ps
            };

            GlobalPos::new(
                r(size.x),
                r(size.y),
                r(size.z)
            )
        };

        let mut overmap = ServerOvermap::new(world_generator, size, Pos3::repeat(0.0));

        for _ in 0..30
        {
            let _chunk = overmap.generate_chunk(random_chunk());
        }
    }
}
