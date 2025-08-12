use std::{fmt, rc::Rc, cell::RefCell};

use super::{
    MarkerTile,
    world_generator::{
        WORLD_CHUNK_SIZE,
        CHUNK_RATIO,
        empty_worldchunk,
        chunk_difficulty,
        ConditionalInfo,
        WorldGenerator,
        WorldChunk
    }
};

use crate::common::{
    Axis,
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

fn debug_overmap_with<S: fmt::Debug, T: fmt::Display>(
    overmap: &ServerOvermap<S>,
    f: impl Fn(Option<&WorldChunk>) -> T
) -> String
{
    let size = overmap.world_chunks.size();

    let position_to_string = |pos: Pos3<usize>|
    {
        let local = LocalPos::new(pos, size);
        let global = overmap.to_global(local);

        format!(
            "(local: {}, {}, {}) | global: {}, {}, {})",
            pos.x, pos.y, pos.z,
            global.0.x, global.0.y, global.0.z
        )
    };

    let longest_position = (0..size.x).map(|x|
    {
        (0..size.y).map(move |y|
        {
            (0..size.z).map(move |z| position_to_string(Pos3::new(x, y, z)).len()).max().unwrap_or(0)
        }).max().unwrap_or(0)
    }).max().unwrap_or(0);

    let size_z = size.z;
    let world_chunks = (0..size_z).fold(String::new(), |mut acc, z|
    {
        overmap.world_chunks.iter_axis(Axis::Z, z).for_each(|(pos, maybe_block)|
        {
            let line = if let Some(block) = maybe_block
            {
                block.iter().fold(String::new(), |mut acc, world_chunk|
                {
                    let value = f(Some(world_chunk)).to_string();

                    acc += &value;

                    acc
                })
            } else
            {
                (0..size_z).fold(String::new(), |mut acc, _|
                {
                    let value = f(None).to_string();

                    acc += &value;

                    acc
                })
            };

            let position_info = format!("{:<longest_position$}: ", position_to_string(pos));

            acc += "\n";
            acc += &position_info;
            acc += &line;
        });

        acc
    });

    let world_plane = overmap.world_plane.0.pretty_print_with(|x| f(x.as_ref()).to_string());

    format!(
        "{:#?}\n{:#?}{world_chunks}\n{world_plane}",
        &overmap.world_generator,
        &overmap.indexer
    )
}

pub struct ServerOvermap<S>
{
    world_generator: Rc<RefCell<WorldGenerator<S>>>,
    world_chunks: ChunksContainer<Option<WorldChunksBlock>>,
    world_plane: WorldPlane,
    indexer: Indexer
}

impl<S: fmt::Debug> fmt::Debug for ServerOvermap<S>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", debug_overmap_with(self, |x|
        {
            if let Some(x) = x
            {
                let id = x.id();
                if x.tags().is_empty()
                {
                    format!("({id})")
                } else
                {
                    format!("({id} {:?})", x.tags())
                }
            } else
            {
                "_".to_owned()
            }
        }))
    }
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

    pub fn move_to(&mut self, pos: GlobalPos)
    {
        let pos = worldchunk_pos(pos);

        let shift_offset = (pos - self.player_position()).0;

        if shift_offset != Pos3::repeat(0_i32)
        {
            self.shift_overmap_by(shift_offset);
        }
    }

    pub fn generate_chunk(&mut self, pos: GlobalPos, marker: impl FnMut(MarkerTile)) -> Chunk
    {
        if let Some(local_pos) = self.to_local(pos)
        {
            self.generate_existing_chunk(local_pos, marker)
        } else
        {
            eprintln!(
                "trying to generate chunk at position {pos:?}, which is outside the server overmap (player position {:?})",
                self.player_position()
            );

            Chunk::new()
        }
    }

    fn shift_overmap_by(&mut self, shift_offset: Pos3<i32>)
    {
        self.indexer.player_position += shift_offset;

        self.position_offset(shift_offset);
    }

    fn generate_existing_chunk(&self, local_pos: LocalPos, mut marker: impl FnMut(MarkerTile)) -> Chunk
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

                    let world_chunk = if let Some(group) = local_pos.always_group()
                    {
                        let group = group.map(|position|
                        {
                            self.world_chunks[position].as_ref().map(|chunk| chunk[z].clone()).unwrap()
                        });

                        let tags = self.world_plane.world_chunk(
                            LocalPos::new(Pos3{z: 0, ..local_pos.pos}, Pos3{z: 1, ..local_pos.size})
                        ).tags();

                        let mut world_generator = self.world_generator.borrow_mut();

                        let info = {
                            let global_pos = self.to_global(local_pos);

                            ConditionalInfo{
                                height: global_pos.0.z * CHUNK_RATIO.z as i32 + z as i32,
                                difficulty: chunk_difficulty(global_pos),
                                rotation: world_generator.rotation_of(group.this.id()),
                                tags
                            }
                        };

                        let mut marker = |mut marker_tile: MarkerTile|
                        {
                            marker_tile.pos.pos_mut().z = z;
                            marker(marker_tile)
                        };

                        world_generator.generate_chunk(
                            &info,
                            group,
                            &mut marker
                        )
                    } else
                    {
                        eprintln!("chunk most not be touching edges: {local_pos:?} (player position {:?})", self.player_position());
                        empty_worldchunk()
                    };

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

    fn generate_missing_inner(&mut self, offset: Option<Pos3<i32>>, surface_override: bool)
    {
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

                self.world_generator.borrow_mut()
                    .generate_surface(surface_blocks, &mut self.world_plane, &outside_indexer)
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
                debug_assert!(!surface_override);

                let new_offset = Pos3::new(0, 0, -self.indexer.player_position().0.z);

                self.indexer.player_position += new_offset;
                self.shift_chunks(new_offset);

                self.generate_missing_inner(None, true);

                self.indexer.player_position -= new_offset;
                self.shift_chunks(-new_offset);
            }
        }

        self.world_generator.borrow_mut()
            .generate_missing(&mut self.world_chunks, &self.world_plane, &self.indexer);
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

    fn generate_missing(&mut self, offset: Option<Pos3<i32>>)
    {
        self.generate_missing_inner(offset, false)
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
        common::{TileMap, world::TileRotation},
        server::world::SERVER_OVERMAP_SIZE_Z
    };


    struct TestSaver<T>
    {
        data: HashMap<GlobalPos, T>
    }

    impl<T> fmt::Debug for TestSaver<T>
    {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
        {
            write!(f, "")
        }
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
            let _chunk = overmap.generate_chunk(random_chunk(), |_| {});
        }
    }

    #[ignore]
    #[test]
    fn moving_around_pillars()
    {
        let saver = TestSaver::new();

        let tiles = "tiles/tiles.json";

        let tilemap = TileMap::parse(tiles, "textures/tiles/").unwrap().tilemap;

        let world_generator = Rc::new(RefCell::new(
            WorldGenerator::new(saver, Rc::new(tilemap), "world_generation_test/").unwrap()
        ));

        let size = Pos3::new(10, 11, SERVER_OVERMAP_SIZE_Z);

        let random_chunk = ||
        {
            let r = |s: usize|
            {
                let ps = (s as i32).pow(2);

                fastrand::i32(0..(ps * 2)) - ps
            };

            GlobalPos::new(
                r(size.x),
                r(size.y),
                r(size.z)
            )
        };

        let mut overmap = ServerOvermap::new(
            world_generator.clone(),
            size,
            Pos3::repeat(0.0)
        );

        let (a_id, c_id, none_id) = {
            let world_generator = world_generator.borrow();
            let names = &world_generator.rules().name_mappings().world_chunk;
            let get_name = |name: &str| names[&(TileRotation::Up, name.to_owned())];

            (get_name("a"), get_name("c"), get_name("none"))
        };

        let overmap_format = |x: Option<&WorldChunk>|
        {
            x.map(|x|
            {
                let id = x.id();

                let world_generator = world_generator.borrow();
                let name = world_generator.rules().name(id);

                match name
                {
                    "none" => '_',
                    "a" => 'O',
                    "b" => 'X',
                    "c" => '*',
                    x => panic!("unhandled name: {x}")
                }
            }).unwrap_or('?')
        };

        let mut move_to = |pos|
        {
            eprintln!("moving to {pos:?}");
            let _chunk = overmap.generate_chunk(pos, |_| {});

            (0..size.x).for_each(|x|
            {
                (0..size.y).for_each(|y|
                {
                    let mut pillar_type: Option<WorldChunk> = None;
                    (0..size.z).for_each(|z|
                    {
                        let local_pos = LocalPos::new(Pos3::new(x, y, z), size);
                        let global_pos = overmap.to_global(local_pos);

                        let current = overmap.get(global_pos);

                        if let Some(Some(current)) = current
                        {
                            current.into_iter().for_each(|world_chunk|
                            {
                                if let Some(expected_pillar) = pillar_type.as_ref()
                                {
                                    let correct_follow = match (expected_pillar, world_chunk)
                                    {
                                        (a, b) if a.id() == a_id && b.id() == none_id => true,
                                        (a, _b) if a.id() == c_id => true,
                                        (a, b) if a == b => true,
                                        (a, b) =>
                                        {
                                            eprintln!("({global_pos:?}) got incorrect follow: {a:?} -> {b:?}");
                                            false
                                        }
                                    };

                                    assert!(
                                        correct_follow,
                                        "{}",
                                        debug_overmap_with(&overmap, overmap_format)
                                    );
                                } else
                                {
                                    pillar_type = Some(world_chunk.clone());
                                }
                            });
                        }
                    })
                });
            });
        };

        move_to(GlobalPos::new(0, 0, 0));

        for _ in 0..1000
        {
            let pos = random_chunk();

            move_to(pos)
        }
    }
}
