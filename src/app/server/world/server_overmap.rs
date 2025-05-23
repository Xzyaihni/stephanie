use std::{rc::Rc, cell::RefCell};

use super::world_generator::{
    WORLD_CHUNK_SIZE,
    CHUNK_RATIO,
    ConditionalInfo,
    WorldGenerator,
    WorldChunk
};

use crate::common::{
    SaveLoad,
    WorldChunksBlock,
    world::{
        CHUNK_SIZE,
        LocalPos,
        GlobalPos,
        Pos3,
        Chunk,
        ChunkLocal,
        chunk::tile::Tile,
        overmap::{
            Overmap,
            OvermapIndexing,
            CommonIndexing,
            FlatChunksContainer,
            ChunksContainer
        }
    }
};


#[derive(Debug, Clone)]
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

impl CommonIndexing for Indexer
{
    fn size(&self) -> Pos3<usize>
    {
        self.size
    }
}

impl OvermapIndexing for Indexer
{
    fn player_position(&self) -> GlobalPos
    {
        self.player_position
    }
}

#[derive(Debug)]
pub struct WorldPlane(pub FlatChunksContainer<Option<WorldChunk>>);

impl WorldPlane
{
    #[allow(dead_code)]
    pub fn all_exist(&self) -> bool
    {
        self.0.iter().all(|x| x.1.is_some())
    }

    pub fn world_chunk(&self, pos: LocalPos) -> &WorldChunk
    {
        // flatindexer ignores the z pos, so i dont have to clear it
        self.0[pos].as_ref().expect("worldchunk must exist")
    }
}

fn worldchunk_pos(pos: GlobalPos) -> GlobalPos
{
    pos * GlobalPos::from(Pos3{z: 1, ..CHUNK_RATIO})
}

#[allow(dead_code)]
fn chunk_pos(pos: GlobalPos) -> GlobalPos
{
    let pos = pos.0.zip(CHUNK_RATIO.map(|x| x as i32)).map(|(pos, ratio)|
    {
        if pos < 0
        {
            (pos + 1) / ratio - 1
        } else
        {
            pos / ratio
        }
    });

    GlobalPos::from(pos)
}

#[derive(Debug)]
pub struct ServerOvermap<S>
{
    world_generator: Rc<RefCell<WorldGenerator<S>>>,
    world_chunks: ChunksContainer<Option<WorldChunksBlock>>,
    world_plane: WorldPlane,
    indexer: Indexer
}

// have u heard of a constant clippy?
#[allow(clippy::modulo_one)]
impl<S: SaveLoad<WorldChunksBlock>> ServerOvermap<S>
{
    pub fn new(
        world_generator: Rc<RefCell<WorldGenerator<S>>>,
        size: Pos3<usize>,
        player_position: Pos3<f32>
    ) -> Self
    {
        assert_eq!(CHUNK_SIZE % WORLD_CHUNK_SIZE.x, 0);
        assert_eq!(CHUNK_SIZE % WORLD_CHUNK_SIZE.y, 0);
        assert_eq!(CHUNK_SIZE % WORLD_CHUNK_SIZE.z, 0);

        let size = Pos3::new(CHUNK_RATIO.x, CHUNK_RATIO.y, 1) * size;

        let indexer = Indexer::new(size, player_position.rounded());

        let world_chunks = ChunksContainer::new(size);

        let world_plane = WorldPlane(FlatChunksContainer::new(size));

        let mut this = Self{
            world_generator,
            world_chunks,
            world_plane,
            indexer
        };

        this.generate_missing(None);

        this
    }

    #[allow(dead_code)]
    pub fn inbounds_chunk(&self, pos: GlobalPos) -> bool
    {
        let world_pos = worldchunk_pos(pos);
        for x in 0..CHUNK_RATIO.x
        {
            for y in 0..CHUNK_RATIO.y
            {
                for z in 0..CHUNK_RATIO.z
                {
                    let check_pos = world_pos + GlobalPos::from(Pos3{x, y, z});

                    if self.inbounds(check_pos)
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn generate_chunk(&mut self, pos: GlobalPos) -> Chunk
    {
        let pos = worldchunk_pos(pos);

        let shift_offset = self.over_bounds_with_padding(
            pos,
            Pos3::repeat(1),
            Pos3{z: 1, ..CHUNK_RATIO.map(|x| x as i32)} + 1
        );

        if shift_offset != Pos3::repeat(0_i32)
        {
            self.shift_overmap_by(shift_offset);
        }

        self.generate_existing_chunk(self.to_local(pos).unwrap())
    }

    fn shift_overmap_by(&mut self, shift_offset: Pos3<i32>)
    {
        self.indexer.player_position = self.indexer.player_position + shift_offset;

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
                    let this_pos = Pos3{x, y, z};

                    let local_pos = local_pos + Pos3{x, y, z: 0};

                    let group = local_pos.always_group().expect("chunk must not touch edges");
                    let group = group.map(|position|
                    {
                        self.world_chunks[position].as_ref().map(|chunk| chunk[z].clone()).unwrap()
                    });

                    let tags = self.world_plane.world_chunk(
                        LocalPos::new(Pos3{z: 0, ..local_pos.pos}, Pos3{z: 1, ..local_pos.size})
                    ).tags();

                    let info = ConditionalInfo{
                        height: self.to_global_z(local_pos.pos.z) * CHUNK_RATIO.z as i32 + z as i32,
                        tags
                    };

                    let world_chunk = self.world_generator.borrow_mut().generate_chunk(
                        &info,
                        group
                    );

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
                    chunk[ChunkLocal::from(pos + this_pos)] = world_chunk[this_pos];
                }
            }
        }
    }
}

impl<S: SaveLoad<WorldChunksBlock>> Overmap<Option<WorldChunksBlock>> for ServerOvermap<S>
{
    fn remove(&mut self, pos: LocalPos)
    {
        self.world_chunks[pos] = None;
    }

    fn swap(&mut self, a: LocalPos, b: LocalPos)
    {
        self.world_chunks.swap(a, b);
    }

    fn get_local(&self, pos: LocalPos) -> &Option<WorldChunksBlock>
    {
        &self.world_chunks[pos]
    }

    fn is_empty(&self, pos: LocalPos) -> bool
    {
        self.get_local(pos).is_none()
    }

    fn get(&self, pos: GlobalPos) -> Option<&Option<WorldChunksBlock>>
    {
        self.to_local(pos).map(|local_pos| self.get_local(local_pos))
    }

    fn contains(&self, pos: GlobalPos) -> bool
    {
        self.get(pos).map(|x| x.is_some()).unwrap_or(false)
    }

    fn mark_ungenerated(&mut self, _pos: LocalPos) {}

    fn generate_missing(&mut self, offset: Option<Pos3<i32>>)
    {
        let mut generator = self.world_generator.borrow_mut();

        let update_surface = if let Some(offset) = offset
        {
            offset.x != 0 || offset.y != 0
        } else
        {
            true
        };

        if update_surface
        {
            let z = 0;
            let local_z = self.to_local_z(z);

            let mut generate_surface = |surface_blocks|
            {
                let mut outside_indexer = self.indexer.clone();
                outside_indexer.player_position.0.z = 0;
                outside_indexer.size.z = 1;

                generator.generate_surface(surface_blocks, &mut self.world_plane, &outside_indexer)
            };

            if let Some(local_z) = local_z
            {
                let mut surface_blocks = self.world_chunks.map_slice_ref(local_z, |(_pos, chunk)| chunk.clone());
                generate_surface(&mut surface_blocks);

                surface_blocks.into_iter().for_each(|(pos, block)|
                {
                    let pos = LocalPos::new(Pos3{z: local_z, ..pos.pos}, Pos3{z: self.indexer.size.z, ..pos.size});
                    self.world_chunks[pos] = block;
                });
            } else
            {
                let mut surface_blocks = FlatChunksContainer::new(self.world_chunks.size());

                generate_surface(&mut surface_blocks);
            }
        }

        generator.generate_missing(&mut self.world_chunks, &self.world_plane, &self.indexer);
    }
}

impl<S> CommonIndexing for ServerOvermap<S>
{
    fn size(&self) -> Pos3<usize>
    {
        self.indexer.size
    }
}

impl<S> OvermapIndexing for ServerOvermap<S>
{
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

    use crate::{
        common::TileMap,
        server::world::SERVER_OVERMAP_SIZE_Z
    };


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

        let world_generator = Rc::new(RefCell::new(
            WorldGenerator::new(saver, Rc::new(tilemap), "world_generation/").unwrap()
        ));

        let size = Pos3::new(10, 11, SERVER_OVERMAP_SIZE_Z);

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

        let mut overmap = ServerOvermap::new(
            world_generator,
            size,
            Pos3::repeat(0.0)
        );

        for _ in 0..30
        {
            let _chunk = overmap.generate_chunk(random_chunk());
        }
    }
}
