use std::{
    fs,
    io,
    convert,
    cell::RefCell,
    fmt::{self, Display, Debug},
    rc::Rc,
    ops::{Index, IndexMut},
    cmp::Ordering,
    collections::HashMap,
    path::{Path, PathBuf}
};

use nalgebra::Vector2;

use crate::{
    debug_config::*,
    common::{
        get_env_value,
        with_error,
        write_log,
        write_log_ln,
        DebugRaw,
        SeededRandom,
        TileMap,
        SaveLoad,
        WeightedPicker,
        WorldChunksBlock,
        lisp::{self, *},
        world::{
            SizeTensor,
            Pos2,
            CheckedPos,
            LocalPos,
            GlobalPos,
            ChunkLocal,
            overmap::{
                CommonIndexing,
                OvermapDimension2,
                OvermapIndexing,
                OvermapIndexing3d,
                FlatIndexer,
                FlatChunksContainer,
                ChunksContainer
            },
            chunk::{
                PosDirection,
                tile::{Tile, TileRotation}
            }
        }
    }
};

#[allow(unused_imports)]
use crate::common::world::{Pos3, overmap::OvermapDimension3};

use super::{
    SERVER_OVERMAP_SIZE_Z,
    MarkerTile,
    MarkerKind
};

pub use super::server_overmap::{Indexer, WorldPlane};

pub use chunk_rules::{
    WORLD_CHUNK_SIZE,
    CHUNK_RATIO,
    ChunkRules,
    ChunkRulesGroup,
    ConditionalInfo,
    WorldChunk,
    WorldChunkId,
    WorldChunkTag
};

mod chunk_rules;


const ENTROPY_EDGE: usize = 3;

pub fn empty_worldchunk() -> ChunksContainer<Tile>
{
    ChunksContainer::new(WORLD_CHUNK_SIZE)
}

fn log_worldchunks(
    label: impl Into<String>,
    world_chunks: &FlatChunksContainer<Option<WorldChunk>>,
    mut edges: impl FnMut(LocalPos<Pos2<usize>>) -> Option<WorldChunkId>
)
{
    let world_size = world_chunks.size();
    let entropy_size = world_size + Pos2::repeat(ENTROPY_EDGE * 2);
    let edge_chunks: Vec<(Pos2<i32>, WorldChunkId)> = FlatIndexer::new(entropy_size).positions().filter_map(|local_pos|
    {
        let pos = local_pos.pos.map(|x| x as i32) - Pos2::repeat(ENTROPY_EDGE as i32);

        if !(0..world_size.x as i32).contains(&pos.x) || !(0..world_size.y as i32).contains(&pos.y)
        {
            return None;
        }

        edges(local_pos).map(|x| (local_pos.pos.map(|x| x as i32), x))
    }).collect();

    let info = (world_chunks.clone(), edge_chunks);

    if let Some(world_chunks_json) = with_error(serde_json::to_string(&info))
    {
        write_log(label);
        write_log_ln(":");
        write_log_ln(world_chunks_json);
    }
}

#[derive(Debug)]
pub enum ParseErrorKind
{
    Io(io::Error),
    Json(serde_json::Error),
    Lisp(lisp::ErrorPos)
}

impl fmt::Display for ParseErrorKind
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let s = match self
        {
            Self::Io(x) => x.to_string(),
            Self::Json(x) => x.to_string(),
            Self::Lisp(x) => x.to_string()
        };

        write!(f, "{s}")
    }
}

impl From<io::Error> for ParseErrorKind
{
    fn from(value: io::Error) -> Self
    {
        ParseErrorKind::Io(value)
    }
}

impl From<serde_json::Error> for ParseErrorKind
{
    fn from(value: serde_json::Error) -> Self
    {
        ParseErrorKind::Json(value)
    }
}

impl From<lisp::ErrorPos> for ParseErrorKind
{
    fn from(value: lisp::ErrorPos) -> Self
    {
        ParseErrorKind::Lisp(value)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ParseError
{
    filename: Option<PathBuf>,
    kind: ParseErrorKind
}

impl fmt::Display for ParseError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let err = &self.kind;

        if let Some(filename) = &self.filename
        {
            let filename = filename.display();

            write!(f, "error at {filename}: {err}")
        } else
        {
            write!(f, "{err}")
        }
    }
}

impl ParseError
{
    pub fn new_named<K: Into<ParseErrorKind>>(filename: PathBuf, kind: K) -> Self
    {
        Self{filename: Some(filename), kind: kind.into()}
    }

    pub fn new<K: Into<ParseErrorKind>>(kind: K) -> Self
    {
        Self{filename: None, kind: kind.into()}
    }
}

impl From<io::Error> for ParseError
{
    fn from(value: io::Error) -> Self
    {
        ParseError::new(value)
    }
}

impl From<lisp::ErrorPos> for ParseError
{
    fn from(value: lisp::ErrorPos) -> Self
    {
        ParseError::new(value)
    }
}

pub fn chunk_difficulty(pos: GlobalPos<Pos2<i32>>) -> f32
{
    let p: Vector2<f32> = Vector2::from(pos.0).cast();

    p.magnitude() * 0.03
}

pub enum ChunkGenerationError
{
    SymbolAllocation(String, lisp::Error),
    TagSymbolAllocation(lisp::Error),
    LispRuntime{source: &'static str, err: lisp::ErrorPos},
    WrongOutput(lisp::Error),
    WrongSize{expected: usize, got: usize}
}

impl Display for ChunkGenerationError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::SymbolAllocation(name, err) => write!(f, "error allocating {name} symbol: {err}"),
            Self::TagSymbolAllocation(err) => write!(f, "error allocating tag symbol: {err}"),
            Self::LispRuntime{source, err} => write!(f, "(in {source}) {err}"),
            Self::WrongOutput(err) => write!(f, "expected vector: {err}"),
            Self::WrongSize{expected, got} => write!(f, "expected vector with {expected} elements, got {got}")
        }
    }
}

pub struct ChunkGenerator
{
    chunks: HashMap<String, Lisp>,
    rules: Rc<ChunkRulesGroup>,
    tilemap: Rc<TileMap>,
    overmaps_world_chunks: Rc<RefCell<Vec<(Rc<RefCell<Indexer>>, Rc<RefCell<WorldPlane>>)>>>
}

impl ChunkGenerator
{
    pub fn new(
        tilemap: Rc<TileMap>,
        rules: Rc<ChunkRulesGroup>,
        parent_directory: PathBuf
    ) -> Result<Self, ParseError>
    {
        let chunks = HashMap::new();

        let overmaps_world_chunks = Rc::new(RefCell::new(Vec::new()));
        let primitives = Rc::new(Self::default_primitives(&tilemap, rules.clone(), overmaps_world_chunks.clone(), false));

        let memory = LispMemory::new(primitives, 256, 1 << 13);

        let mut this = Self{
            chunks,
            rules: rules.clone(),
            tilemap,
            overmaps_world_chunks
        };

        rules.iter_names().filter(|(rotation, name)|
        {
            let name: &str = name.as_ref();

            *rotation == TileRotation::Up && name != "none"
        }).for_each(|(_, name)|
        {
            if let Err(err) = this.parse_function(&parent_directory, memory.clone(), name)
            {
                eprintln!("{err}");

                #[cfg(test)]
                {
                    panic!();
                }
            }
        });

        Ok(this)
    }

    pub fn default_primitives(
        tilemap: &TileMap,
        rules: Rc<ChunkRulesGroup>,
        overmaps_world_chunks: Rc<RefCell<Vec<(Rc<RefCell<Indexer>>, Rc<RefCell<WorldPlane>>)>>>,
        allow_out_of_range: bool
    ) -> Primitives
    {
        let mut primitives = Primitives::default();

        let fallback_tile = Tile::none();
        let names_map: HashMap<String, Tile> = tilemap.names_owned_map();

        primitives.add(
            "allow-out-of-range-chunks",
            PrimitiveProcedureInfo::new_simple(0, Effect::Pure, move |_args|
            {
                Ok(allow_out_of_range.into())
            }));

        primitives.add(
            "tile",
            PrimitiveProcedureInfo::new_simple(
                ArgsCount::Min(1),
                Effect::PureIf(PureCondition::ArgsBetween{start: 1, end_inclusive: 1}),
                move |mut args|
                {
                    let call_position = args.call_position();

                    let name = args.next().unwrap().as_symbol(args.memory)?;
                    let rotation = args.next();

                    let mut tile = *names_map.get(&name).unwrap_or_else(||
                    {
                        eprintln!("no tile named `{name}`, using fallback");

                        #[cfg(test)]
                        {
                            panic!();
                        }

                        #[allow(unreachable_code)]
                        &fallback_tile
                    });

                    if let Some(rotation) = rotation
                    {
                        if let Err(err) = || -> Result<(), lisp::Error>
                        {
                            let rotation = TileRotation::from_lisp_value(rotation)?;

                            tile.0.as_mut().ok_or_else(||
                            {
                                lisp::Error::Custom("air cannot have rotation".to_owned())
                            })?.set_rotation(rotation);

                            Ok(())
                        }()
                        {
                            let err: lisp::ErrorPos = err.with_position(call_position);
                            eprintln!("{err}, ignoring");

                            #[cfg(test)]
                            {
                                panic!();
                            }
                        }
                    }

                    let lisp_value = tile.as_lisp_value(args.memory);

                    if rotation.is_none()
                    {
                        if let Ok(lisp_value) = lisp_value
                        {
                            debug_assert!(!lisp_value.tag().is_boxed());
                        }
                    }

                    lisp_value
                }));

        fn read_chunk_position(
            args: &mut PrimitiveArgs,
        ) -> Result<(Pos2<usize>, usize), lisp::Error>
        {
            let values = args.next().unwrap().as_pairs_list(args.memory)?;

            if values.len() != 3
            {
                return Err(lisp::Error::Custom("chunk position is malformed".to_owned()));
            }

            let mut values = values.into_iter();

            let overmap_index = values.next().unwrap().as_integer()? as usize;
            let x = values.next().unwrap().as_integer()? as usize;
            let y = values.next().unwrap().as_integer()? as usize;

            Ok((Pos2{x, y}, overmap_index))
        }

        {
            let overmaps_world_chunks = overmaps_world_chunks.clone();

            primitives.add(
                "difficulty-at",
                PrimitiveProcedureInfo::new_simple(1, Effect::Pure, move |mut args|
                {
                    let global_pos = {
                        let (position, overmap_index) = read_chunk_position(&mut args)?;

                        let local_pos_unconverted = GlobalPos(position.map(|x| x as i32));

                        let overmaps = overmaps_world_chunks.borrow();

                        let (indexer, _) = overmaps.get(overmap_index).ok_or_else(||
                        {
                            lisp::Error::Custom(format!("overmap index {overmap_index} doesnt exist"))
                        })?;

                        let indexer = indexer.borrow();

                        OvermapIndexing::<OvermapDimension2>::to_global_unconverted(&*indexer, local_pos_unconverted)
                    };

                    Ok(chunk_difficulty(global_pos).into())
                }));
        }

        fn read_chunk_info<T>(
            overmaps_world_chunks: &[(Rc<RefCell<Indexer>>, Rc<RefCell<WorldPlane>>)],
            args: &mut PrimitiveArgs,
            allow_out_of_range: bool,
            f: impl FnOnce(&mut PrimitiveArgs, &WorldChunk) -> T
        ) -> Result<T, lisp::Error>
        {
            let (Pos2{x, y}, overmap_index) = read_chunk_position(args)?;

            let (_, this_overmap) = overmaps_world_chunks.get(overmap_index).ok_or_else(||
            {
                lisp::Error::Custom(format!("overmap index {overmap_index} doesnt exist"))
            })?;

            let this_overmap = this_overmap.borrow();

            let pos = Pos2::new(x, y);

            let this_chunk = this_overmap.0.get(pos);

            let output = if let Some(x) = this_chunk
            {
                if let Some(x) = x.as_ref()
                {
                    f(args, x)
                } else
                {
                    if allow_out_of_range
                    {
                        f(args, &WorldChunk::default())
                    } else
                    {
                        return Err(lisp::Error::Custom("world chunk block isnt generated, this isnt supposed to happen ever".to_owned()));
                    }
                }
            } else
            {
                if allow_out_of_range
                {
                    f(args, &WorldChunk::default())
                } else
                {
                    return Err(lisp::Error::Custom(format!("{pos} is out of range")));
                }
            };

            Ok(output)
        }

        {
            let overmaps_world_chunks = overmaps_world_chunks.clone();
            let rules = rules.clone();

            primitives.add(
                "chunk-at",
                PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
                {
                    let world_chunk = {
                        let overmaps_world_chunks = overmaps_world_chunks.borrow();

                        read_chunk_info(&overmaps_world_chunks, &mut args, allow_out_of_range, |_, x| x.id())?
                    };

                    let (rotation, name) = rules.name_mappings().world_chunk.get_back(&world_chunk).unwrap();

                    let memory = args.memory;

                    let restore = memory.with_saved_registers([Register::Value, Register::Temporary]);

                    memory.set_register(Register::Value, *rotation as i32);

                    let name = memory.new_symbol(name);
                    memory.set_register(Register::Temporary, name);

                    memory.cons(Register::Value, Register::Temporary, Register::Value)?;

                    let value = memory.get_register(Register::Value);

                    restore(memory)?;

                    Ok(value)
                }));
        }

        primitives.add(
            "chunk-tags-at",
            PrimitiveProcedureInfo::new_simple(1, Effect::Impure, move |mut args|
            {
                let overmaps_world_chunks = overmaps_world_chunks.borrow();

                let name_mappings = rules.name_mappings();

                read_chunk_info(&overmaps_world_chunks, &mut args, allow_out_of_range, |args, x|
                {
                    let tag_values: Vec<_> = x.tags().iter().filter_map(|tag|
                    {
                        with_error(tag.as_lisp_value(name_mappings, args.memory))
                    }).collect();

                    args.memory.cons_list(tag_values)
                }).flatten()
            }));

        primitives
    }

    fn push_world_chunks(&self, indexer: Rc<RefCell<Indexer>>, world_chunks: Rc<RefCell<WorldPlane>>)
    {
        self.overmaps_world_chunks.borrow_mut().push((indexer, world_chunks));
    }

    fn parse_function(
        &mut self,
        parent_directory: &Path,
        memory: LispMemory,
        name: &str
    ) -> Result<(), ParseError>
    {
        fn load(name: impl AsRef<Path>) -> String
        {
            let name = name.as_ref();
            fs::read_to_string(name)
                .unwrap_or_else(|err| panic!("{} must exist >_< ({err})", name.display()))
        }

        let chunks_directory = parent_directory.join("chunks");
        let filepath = chunks_directory.join(format!("{name}.scm"));

        let standard_code = load("lisp/standard.scm");
        let default_code = load(parent_directory.join("default.scm"));
        let chunk_code = fs::read_to_string(&filepath).map_err(|err|
        {
            // cant remove the clone cuz ? is cringe or something
            ParseError::new_named(filepath.clone(), err)
        })?;

        let config = LispConfig{
            load_handler: {
                let parent_directory = chunks_directory;
                Some(Box::new(move |filename|
                {
                    match fs::read_to_string(parent_directory.join(filename))
                    {
                        Ok(x) => Some(x),
                        Err(err) =>
                        {
                            eprintln!("error trying to load `{filename}`: {err}");

                            #[cfg(test)]
                            {
                                panic!();
                            }

                            #[allow(unreachable_code)]
                            None
                        }
                    }
                }))
            },
            memory,
            env_variables: vec!["height".to_owned(), "difficulty".to_owned(), "rotation".to_owned(), "position".to_owned()],
            ..Default::default()
        };

        if DebugConfig::is_enabled(DebugTool::Lisp)
        {
            eprintln!("compiling {name} with {:?}", config.compile_config);
        }

        let lisp = Lisp::new_with_config(config, &[&standard_code, &default_code, &chunk_code]).map_err(|err|
        {
            ParseError::new_named(PathBuf::from(name), err)
        })?;

        self.chunks.insert(name.to_owned(), lisp);

        Ok(())
    }

    pub fn generate_chunk_with(
        info: &ConditionalInfo,
        chunk_name: &str,
        rotation: TileRotation,
        overmap_index: i32,
        this_chunk: &mut Lisp,
        marker: &mut impl FnMut(MarkerTile)
    ) -> Result<ChunksContainer<Tile>, ChunkGenerationError>
    {
        this_chunk.memory_mut().clear();

        let tiles = {
            let define_symbol_with = |memory: &mut LispMemory, name, value|
            {
                memory.define(name, value).map_err(|err|
                {
                    ChunkGenerationError::SymbolAllocation(name.to_owned(), err)
                })
            };

            {
                let memory = this_chunk.memory_mut();

                let pos = info.position.pos;

                let chunk_position = memory.cons_list([overmap_index, pos.x as i32, pos.y as i32]).map_err(|err|
                {
                    ChunkGenerationError::SymbolAllocation("position".to_owned(), err)
                })?;

                define_symbol_with(memory, "position", chunk_position)?;
            }

            let mut define_symbol = |name, value|
            {
                define_symbol_with(this_chunk.memory_mut(), name, value)
            };

            define_symbol("height", info.height.into())?;
            define_symbol("difficulty", info.difficulty.into())?;
            define_symbol("rotation", (rotation as i32).into())?;

            let (memory, value): (&LispMemory, LispValue) = this_chunk.run_precleared()
                .map_err(|err|
                {
                    let source = ["standard", "default", "chunk", "loaded file"].get(err.position.source).copied().unwrap_or("undefined");

                    ChunkGenerationError::LispRuntime{source, err}
                })?
                .destructure();

            let output = value.as_vector_ref(memory).map_err(|err|
            {
                ChunkGenerationError::WrongOutput(err)
            })?;

            {
                const _: () = assert!(WORLD_CHUNK_SIZE.z == 1, "i didnt implement rotation for anything other than z 1");
            }

            {
                const _: () = assert!(WORLD_CHUNK_SIZE.x == WORLD_CHUNK_SIZE.y, "cant rotate non square chunks");
            }

            {
                let expected = WORLD_CHUNK_SIZE.product();
                let got = output.len();

                if got != expected
                {
                    return Err(ChunkGenerationError::WrongSize{expected, got});
                }
            }

            fn process<'a>(
                memory: &LispMemory,
                marker: &mut impl FnMut(MarkerTile),
                chunk_name: &str,
                rotation: TileRotation,
                values: impl Iterator<Item=&'a LispValue>
            ) -> Box<[Tile]>
            {
                fn index_to_pos(index: usize) -> ChunkLocal
                {
                    ChunkLocal::new(index % WORLD_CHUNK_SIZE.x, index / WORLD_CHUNK_SIZE.x, 0)
                }

                values.enumerate().map(|(index, x)|
                {
                    || -> Result<_, _>
                    {
                        let x = OutputWrapperRef::new(memory, *x);
                        if let Ok(s) = x.as_list().and_then(|lst| lst.car.as_symbol())
                        {
                            if s != "marker"
                            {
                                return Err(lisp::Error::Custom(format!("malformed tile, expected marker got {s}")));
                            }

                            let value = x.as_list().unwrap().cdr;

                            let pos = index_to_pos(index);
                            MarkerKind::from_lisp_value(value)?.into_iter().for_each(|marker_tile|
                            {
                                marker(MarkerTile{kind: marker_tile.rotated(rotation), pos});
                            });

                            Ok(Tile::none())
                        } else
                        {
                            Tile::from_lisp_value(x)
                        }
                    }().unwrap_or_else(|err|
                    {
                        let pos = *index_to_pos(index).pos();

                        eprintln!("tile error at ({}, {}) in ({chunk_name}): {err}, using fallback", pos.x, pos.y);

                        #[cfg(test)]
                        {
                            panic!();
                        }

                        #[allow(unreachable_code)]
                        Tile::none()
                    })
                }).collect::<Box<[Tile]>>()
            }

            const SPAN: usize = WORLD_CHUNK_SIZE.x;
            match rotation
            {
                TileRotation::Up => process(memory, marker, chunk_name, rotation, output.iter()),
                TileRotation::Right =>
                {
                    let values = (0..SPAN).flat_map(|x|
                    {
                        (0..SPAN).rev().map(move |y| y * SPAN + x)
                    }).map(|index| &output[index]);

                    process(memory, marker, chunk_name, rotation, values)
                },
                TileRotation::Left =>
                {
                    let values = (0..SPAN).rev().flat_map(|x|
                    {
                        (0..SPAN).map(move |y| y * SPAN + x)
                    }).map(|index| &output[index]);

                    process(memory, marker, chunk_name, rotation, values)
                },
                TileRotation::Down => process(memory, marker, chunk_name, rotation, output.iter().rev())
            }
        };

        Ok(ChunksContainer::from_raw(WORLD_CHUNK_SIZE, tiles))
    }

    pub fn generate_chunk(
        &mut self,
        info: &ConditionalInfo,
        this_chunk: WorldChunkId,
        chunk_name: &str,
        overmap_index: i32,
        marker: &mut impl FnMut(MarkerTile)
    ) -> ChunksContainer<Tile>
    {

        let rotation = self.rules.rotation(this_chunk);

        let this_chunk = if let Some(x) = self.chunks.get_mut(chunk_name)
        {
            x
        } else
        {
            eprintln!("worldchunk named `{chunk_name}` doesnt exist");

            #[cfg(test)]
            {
                panic!();
            }

            #[allow(unreachable_code)]
            return empty_worldchunk();
        };

        match Self::generate_chunk_with(info, chunk_name, rotation, overmap_index, this_chunk, marker)
        {
            Ok(x) => x,
            Err(err) =>
            {
                eprintln!("{err} in ({chunk_name}, at {}), using fallback", info.position.pos);

                #[cfg(test)]
                {
                    let planes = self.overmaps_world_chunks.borrow();
                    let this_plane = planes[overmap_index as usize].1.borrow();

                    log_worldchunks("chunk error", &this_plane.0, |_| None);

                    panic!();
                }

                #[allow(unreachable_code)]
                empty_worldchunk()
            }
        }
    }
}

impl fmt::Debug for ChunkGenerator
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        f.debug_struct("ChunkGenerator")
            .field("tilemap", &self.tilemap)
            .finish()
    }
}

#[derive(Debug)]
pub struct WorldGenerator<S>
{
    world_seed: u64,
    generator: ChunkGenerator,
    saver: S,
    rules: Rc<ChunkRulesGroup>
}

impl<S: SaveLoad<WorldChunksBlock>> WorldGenerator<S>
{
    pub fn new(
        saver: S,
        world_seed: u64,
        tilemap: Rc<TileMap>,
        path: impl Into<PathBuf>
    ) -> Result<Self, ParseError>
    {
        let path = path.into();
        let rules = Rc::new(ChunkRulesGroup::load(path.clone())?);

        let generator = ChunkGenerator::new(tilemap, rules.clone(), path)?;

        Ok(Self{world_seed, generator, saver, rules})
    }

    #[cfg(debug_assertions)]
    pub fn get_debug(&mut self, pos: GlobalPos) -> Option<WorldChunksBlock>
    {
        self.saver.load(pos)
    }

    pub fn push_world_chunks(&self, indexer: Rc<RefCell<Indexer>>, world_chunks: Rc<RefCell<WorldPlane>>)
    {
        self.generator.push_world_chunks(indexer, world_chunks)
    }

    pub fn generate_surface<M: OvermapIndexing<OvermapDimension2> + OvermapIndexing + Debug>(
        &mut self,
        mut rng: SeededRandom,
        world_chunks: &mut ChunksContainer<Option<WorldChunksBlock>>,
        plane: &mut WorldPlane,
        global_mapper: &M
    )
    {
        #[cfg(debug_assertions)]
        {
            let chunk_positions: Vec<_> = world_chunks.iter()
                .map(|(pos, _)| pos)
                .collect();

            debug_assert!(
                chunk_positions.iter().all(|pos| OvermapIndexing::<OvermapDimension3>::to_global(global_mapper, *pos).0.z == 0),
                "z must be 0, {global_mapper:#?} {chunk_positions:#?}"
            );
        }

        crate::debug_time_this!{"load-surface-missing", self.load_missing(world_chunks.iter_mut(), global_mapper)}

        let mut any_empty = false;
        plane.0.iter_mut().zip(world_chunks.iter()).for_each(|((_, plane), (_, world))|
        {
            *plane = world.as_ref().map(|chunk| chunk[0].clone());

            if plane.is_none()
            {
                any_empty = true;
            }
        });

        if !any_empty
        {
            return;
        }

        let rng = SeededRandom::from(rng.next_u64().wrapping_add(self.world_seed));

        let saver = &mut self.saver;

        let mut wave_collapser = crate::debug_time_this!{
            "wfc-new",
            WaveCollapser::new(rng, &self.rules.surface, &mut plane.0, global_mapper, move |global_pos|
            {
                saver.load(global_pos.with_z(0)).map(|chunk| chunk[0].id())
            })
        };

        if let Some(local) = OvermapIndexing::<OvermapDimension2>::to_local(global_mapper, GlobalPos::new_2d(0, 0))
        {
            wave_collapser.generate_single_maybe(local, ||
            {
                let random_rotation = TileRotation::random();
                self.rules.name_mappings().world_chunk.get(&(random_rotation, "bunker".to_owned())).map(|bunker_id|
                {
                    WorldChunk::new(*bunker_id, Vec::new())
                }).unwrap_or_else(|| WorldChunk::new(self.rules.surface.fallback(), Vec::new()))
            });
        }

        fn print_worldchunks(wave_collapser: &WaveCollapser)
        {
            eprintln!("{}", wave_collapser.world_chunks.pretty_print_with(|maybe_world_chunk|
            {
                maybe_world_chunk.as_ref().map(|x| wave_collapser.rules.format_id(x.id())).unwrap_or_else(|| "_".to_owned())
            }));
        }

        if DebugConfig::is_enabled(DebugTool::PrintWfcStability)
        {
            let times = get_env_value("STEPHANIE_WFCSAMPLES").unwrap_or(200);
            let success_times = crate::debug_time_this!{"wfc-stability",
            {
                (0..times)
                    .map(|_| wave_collapser.with_cloned(|mut x|
                    {
                        x.rng = SeededRandom::new();

                        x.generate(false)
                    }))
                    .filter(|x| *x)
                    .count()
            }};

            let success_percent = success_times as f32 / times as f32 * 100.0;

            print_worldchunks(&wave_collapser);
            log_worldchunks(format!("this has {success_percent:.1}% success rate"), wave_collapser.world_chunks, |_| None);

            eprintln!("wfc generation success rate: {success_percent:.1}% ({success_times}/{times})");
        }

        let attempts = 10;

        for attempt in 0..attempts
        {
            let is_last_attempt = (attempt + 1) == attempts;

            if cfg!(debug_assertions) && is_last_attempt
            {
                print_worldchunks(&wave_collapser);

                log_worldchunks("this forced fallbacks", wave_collapser.world_chunks, |_| None);
            }

            let wave_collapser_state = wave_collapser.restorable_state();

            let is_success = crate::debug_time_this!{"wfc-generate", wave_collapser.generate(is_last_attempt)};

            if DebugConfig::is_enabled(DebugTool::PrintWfcStability)
            {
                eprintln!("generation {attempt}: {}", if is_success { "success" } else { "failure" });
            }

            if is_success
            {
                break;
            }

            let unique_rng = wave_collapser.rng.clone();
            wave_collapser.restore(wave_collapser_state);
            wave_collapser.rng = unique_rng;
        }
    }

    pub fn generate_missing(
        &mut self,
        world_chunks: &mut ChunksContainer<Option<WorldChunksBlock>>,
        world_plane: &WorldPlane,
        global_mapper: &impl OvermapIndexing3d
    )
    {
        debug_assert!(world_plane.all_exist());
        debug_assert!(world_chunks.size().z == SERVER_OVERMAP_SIZE_Z);
        debug_assert!(global_mapper.size() == world_chunks.size());

        crate::debug_time_this!{"load-all-missing", self.load_missing(world_chunks.iter_mut(), global_mapper)}

        #[cfg(debug_assertions)]
        {
            use crate::debug_config::*;

            if DebugConfig::is_enabled(DebugTool::RedundantWorldChecks)
            {
                world_plane.0.iter().for_each(|(pos, value)|
                {
                    let global_pos = global_mapper.to_global(
                        LocalPos::new(pos.pos.with_z(0), pos.size.with_z(global_mapper.size().z))
                    );

                    if let Some(saved) = self.saver.load(GlobalPos(Pos3{z: 0, ..global_pos.0}))
                    {
                        debug_assert!(
                            saved[0] == value.clone().unwrap(),
                            "{global_pos:?} {:?} != {:?}",
                            saved[0],
                            value.clone().unwrap()
                        );
                    }
                });

                world_chunks.iter().for_each(|(pos, value)|
                {
                    if let Some(saved) = self.saver.load(global_mapper.to_global(pos))
                    {
                        debug_assert!(saved == value.clone().unwrap(), "{saved:?} != {value:?}");
                    }
                });
            }

            if let Some(local_z) = global_mapper.to_local_z(0)
            {
                let s = world_chunks.map_slice_ref(local_z, |(plane_pos, x)|
                {
                    (world_plane.0[plane_pos].clone(), x.as_ref().map(|x| x[0].clone()))
                });

                debug_assert!(
                    s.iter().all(|(_, (a, b))| b.as_ref().map(|b| *a == Some(b.clone())).unwrap_or(true)),
                    "world plane must match the worldchunks: {s:#?}"
                );
            }
        }

        let indexer = FlatIndexer::new(world_chunks.size().into());
        (0..indexer.size().product()).for_each(|index|
        {
            let flat_local = indexer.index_to_pos(index);

            (0..SERVER_OVERMAP_SIZE_Z).rev().for_each(|z|
            {
                let size = world_chunks.size();

                let local_pos = LocalPos::new(flat_local.pos.with_z(z), size);

                let this_world_chunk = &mut world_chunks[local_pos];
                if this_world_chunk.is_some()
                {
                    return;
                }

                let global_pos = global_mapper.to_global(local_pos);

                let difficulty = chunk_difficulty(global_pos.into());

                let block: WorldChunksBlock = (0..CHUNK_RATIO.z).map(|index|
                {
                    let mut global_pos = global_pos;
                    global_pos.0.z = global_pos.0.z * CHUNK_RATIO.z as i32 + index as i32;

                    let global_z = global_pos.0.z;

                    let this_surface = world_plane.world_chunk(flat_local);

                    match global_z.cmp(&0)
                    {
                        Ordering::Equal => this_surface.clone(),
                        Ordering::Greater =>
                        {
                            // above ground

                            let info = ConditionalInfo{
                                position: flat_local,
                                height: global_z,
                                difficulty
                            };

                            self.rules.city.generate(info, this_surface.id())
                        },
                        Ordering::Less =>
                        {
                            // underground

                            let info = ConditionalInfo{
                                position: flat_local,
                                height: global_z,
                                difficulty
                            };

                            let underground_city = self.rules.city.generate_underground(
                                info,
                                this_surface.id()
                            );

                            underground_city.unwrap_or_else(||
                            {
                                WorldChunk::new(self.rules.underground.fallback(), Vec::new())
                            })
                        }
                    }
                }).collect::<Vec<_>>().try_into().unwrap();

                if global_pos.0.z == 0
                {
                    debug_assert!(block[0].id() != WorldChunkId::none());
                }

                self.saver.save(global_pos, block.clone());
                *this_world_chunk = Some(block);
            });
        });

        #[cfg(debug_assertions)]
        if let Some(z) = global_mapper.to_local_z(0)
        {
            let world_chunks_slice = world_chunks.map_slice_ref(z, |(_, v)| v.as_ref().map(|x| x[0].clone()));

            assert!(
                world_chunks_slice.iter().zip(world_plane.0.iter()).all(|(a, b)| a == b),
                "world_chunks: {:#?}, world_plane: {:#?}",
                world_chunks_slice,
                world_plane.0
            );
        }

        debug_assert!(
            world_chunks.iter().all(|(_, x)| x.is_some()),
            "{:?}, empty count: {}, by z: {:#?}",
            world_chunks.size(), world_chunks.iter().filter(|(_, x)| x.is_none()).count(),
            world_chunks.iter().fold(vec![0; world_chunks.size().z], |mut acc, (pos, x)|
            {
                if x.is_none()
                {
                    acc[pos.pos.z] += 1;
                }

                acc
            })
        );
    }

    pub fn rules(&self) -> &ChunkRulesGroup
    {
        &self.rules
    }

    fn load_missing<'a>(
        &mut self,
        world_chunks: impl Iterator<Item=(LocalPos, &'a mut Option<WorldChunksBlock>)>,
        global_mapper: &impl OvermapIndexing
    )
    {
        world_chunks.filter(|(_pos, chunk)| chunk.is_none())
            .for_each(|(pos, chunk)|
            {
                let global_pos = global_mapper.to_global(pos);
                let loaded_chunk = self.saver.load(global_pos);

                if loaded_chunk.is_some()
                {
                    *chunk = loaded_chunk;
                }
            });
    }

    pub fn rotation_of(&self, id: WorldChunkId) -> TileRotation
    {
        self.rules.rotation(id)
    }

    pub fn generate_chunk(
        &mut self,
        info: &ConditionalInfo,
        this_chunk: WorldChunk,
        overmap_index: i32,
        marker: &mut impl FnMut(MarkerTile)
    ) -> ChunksContainer<Tile>
    {
        let id = this_chunk.id();

        debug_assert!(id != WorldChunkId::none());

        let world_chunk_name = self.rules.name(id);

        crate::tool_time_this!{
            format!("world-chunk({world_chunk_name})"),
            DebugTool::DebugChunkTimings,
            self.generator.generate_chunk(info, id, world_chunk_name, overmap_index, marker)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PossibleState
{
    pub id: WorldChunkId,
    pub weight: f64
}

#[derive(Debug, Clone)]
pub struct PossibleStates
{
    states: Vec<PossibleState>,
    total: f64,
    entropy: f64,
    collapsed: bool,
    is_all: bool
}

impl PartialEq for PossibleStates
{
    fn eq(&self, other: &Self) -> bool
    {
        self.states == other.states
    }
}

impl PossibleStates
{
    fn new(rules: &ChunkRules, difficulty: f32) -> Self
    {
        let states: Vec<_> = rules.possible_states(difficulty);

        let entropy = Self::calculate_entropy(states.iter().map(|x| x.weight));

        Self{
            states,
            total: 1.0,
            entropy,
            collapsed: false,
            is_all: true
        }
    }

    fn new_collapsed(id: WorldChunkId) -> Self
    {
        debug_assert!(id != WorldChunkId::none());

        Self{
            states: vec![PossibleState{id, weight: 0.0}],
            total: 1.0,
            entropy: 0.0,
            collapsed: true,
            is_all: false
        }
    }

    fn constrain_both(
        &mut self,
        rules: &ChunkRules,
        other: &mut PossibleStates,
        direction: PosDirection
    ) -> Result<(bool, bool), bool>
    {
        let mut first_changed = false;
        let mut second_changed = false;

        loop
        {
            let changed = self.constrain(rules, other, direction).ok_or(false);

            let this_second_changed = other.constrain(rules, self, direction.opposite()).ok_or(true)?;

            first_changed |= changed?;
            second_changed |= this_second_changed;

            if !this_second_changed
            {
                break;
            }
        }

        Ok((first_changed, second_changed))
    }

    fn constrain(
        &mut self,
        rules: &ChunkRules,
        other: &PossibleStates,
        direction: PosDirection
    ) -> Option<bool>
    {
        if other.is_all() || self.collapsed()
        {
            return Some(false);
        }

        let mut any_constrained = false;

        debug_assert!(!other.states.is_empty());

        let opposite_direction = direction.opposite();

        let mut states_len = self.states.len();
        let mut state_index = 0;

        while state_index < states_len
        {
            let state = &self.states[state_index];

            let this_neighbors = rules.get(state.id).neighbors(opposite_direction);

            if DebugConfig::is_enabled(DebugTool::RedundantWorldChecks)
            {
                debug_assert!(this_neighbors.is_sorted(), "the neighbors must be sorted: {this_neighbors:?}");
            }

            let keep = if this_neighbors.len() == 1
            {
                if DebugConfig::is_enabled(DebugTool::RedundantWorldChecks)
                {
                    debug_assert!(other.states.is_sorted_by_key(|x| x.id), "the states must be sorted: {:?}", &other.states);
                }

                other.states.binary_search_by_key(&this_neighbors[0], |x| x.id).is_ok()
            } else
            {
                other.states.iter().any(|other_state| this_neighbors.binary_search(&other_state.id).is_ok())
            };

            if !keep
            {
                let this_weight = state.weight;

                debug_assert!(this_weight.is_finite(), "illegal weight: {this_weight}");

                if this_weight > 0.0
                {
                    self.total -= this_weight;
                    self.entropy += this_weight * this_weight.ln();
                }

                any_constrained = true;

                self.states.remove(state_index);

                states_len -= 1;
            } else
            {
                state_index += 1;
            }
        }

        if any_constrained
        {
            if self.states.is_empty()
            {
                self.states = vec![PossibleState{id: rules.fallback(), weight: 0.0}];

                return None;
            } else
            {
                self.is_all = false;
                self.update_entropy();
            }
        }

        Some(any_constrained)
    }

    pub fn collapse(&mut self, rules: &ChunkRules, rng: &mut SeededRandom) -> WorldChunkId
    {
        let id = if self.states.is_empty()
        {
            rules.fallback()
        } else if self.collapsed() || (self.states.len() == 1)
        {
            self.states[0].id
        } else
        {
            WeightedPicker::new(self.total, &self.states)
                .pick_with(rng.next_f64(), |value| value.weight)
                .expect("rules cannot be empty")
                .id
        };

        self.set_collapsed_id(id);

        id
    }

    pub fn states(&self) -> &[PossibleState]
    {
        &self.states
    }

    fn set_collapsed_id(&mut self, id: WorldChunkId)
    {
        self.states = vec![PossibleState{id, weight: 0.0}];
        self.collapsed = true;
        self.is_all = false;
    }

    fn update_entropy(&mut self)
    {
        self.entropy = if self.states.len() <= 1
        {
            0.0
        } else
        {
            Self::calculate_entropy(self.states.iter().map(|state| state.weight))
        };
    }

    fn calculate_entropy(weights: impl Iterator<Item=f64>) -> f64
    {
        let entropy: f64 = weights.map(|value|
        {
            if value <= 0.0 { 0.0 } else { -value * value.ln() }
        }).sum();

        debug_assert!(entropy >= 0.0 && entropy.is_finite(), "invalid entropy: {entropy}");

        entropy
    }

    pub fn is_all(&self) -> bool
    {
        self.is_all
    }

    pub fn collapsed(&self) -> bool
    {
        self.collapsed
    }

    pub fn entropy(&self) -> f64
    {
        self.entropy
    }

    #[allow(dead_code)]
    fn format_states(&self, rules: &ChunkRules) -> String
    {
        self.format_states_with(|x| rules.format_id(x))
    }

    fn format_states_with(&self, f: impl Fn(WorldChunkId) -> String) -> String
    {
        let states = self.states.iter().copied().map(|x| f(x.id)).reduce(|acc, x|
        {
            acc + ", " + &x
        }).unwrap_or_default();

        format!("[{states}]")
    }
}

fn edge_unmap(pos: LocalPos<Pos2<usize>>) -> Option<LocalPos<Pos2<usize>>>
{
    let new_size = pos.size - Pos2::repeat(ENTROPY_EDGE * 2);
    let new_pos = pos.pos.map(|x| x as i32 - ENTROPY_EDGE as i32);

    ((0..new_size.x as i32).contains(&new_pos.x) && (0..new_size.y as i32).contains(&new_pos.y)).then(||
    {
        LocalPos::new(new_pos.map(|x| x as usize), new_size)
    })
}

pub fn edge_map_raw(pos: Pos2<i32>) -> Pos2<i32>
{
    pos + Pos2::repeat(ENTROPY_EDGE as i32)
}

fn edge_map_local(mut pos: LocalPos<Pos2<usize>>) -> LocalPos<Pos2<usize>>
{
    pos.size += Pos2::repeat(ENTROPY_EDGE * 2);
    pos.pos += Pos2::repeat(ENTROPY_EDGE);

    pos
}

#[cfg(debug_assertions)]
fn edge_map(pos: LocalPos<Pos2<usize>>) -> LocalPos<Pos2<usize>>
{
    edge_map_local(pos)
}

#[cfg(not(debug_assertions))]
fn edge_map(pos: Pos2<usize>) -> Pos2<usize>
{
    pos + Pos2::repeat(ENTROPY_EDGE)
}

fn chunks_edge_logger(entropies: &Entropies) -> impl Fn(LocalPos<Pos2<usize>>) -> Option<WorldChunkId> + use<'_>
{
    |local_pos|
    {
        let possible_states = &entropies.get(local_pos.into()).states;

        (possible_states.len() == 1).then(||
        {
            possible_states[0].id
        })
    }
}

#[derive(Clone, PartialEq)]
pub struct Entropies(FlatChunksContainer<PossibleStates>);

impl Debug for Entropies
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        f.debug_tuple("Entropies")
            .field(&self.0.map(|x| DebugRaw(x.format_states_with(|x| x.to_string()))))
            .finish()
    }
}

impl Entropies
{
    pub fn size(&self) -> Pos2<usize>
    {
        self.0.size()
    }

    pub fn positions(&self) -> impl Iterator<Item=LocalPos<Pos2<usize>>>
    {
        self.0.positions()
    }

    pub fn get(&self, pos: CheckedPos<Pos2<usize>>) -> &PossibleStates
    {
        &self.0[pos]
    }

    pub fn get_mut(&mut self, pos: CheckedPos<Pos2<usize>>) -> &mut PossibleStates
    {
        &mut self.0[pos]
    }

    fn get_two_mut(
        &mut self,
        one: CheckedPos<Pos2<usize>>,
        two: CheckedPos<Pos2<usize>>
    ) -> (&mut PossibleStates, &mut PossibleStates)
    {
        self.0.get_two_mut(one.into(), two.into())
    }

    pub fn lowest_entropies(&self) -> Vec<LocalPos<Pos2<usize>>>
    {
        let mut lowest_entropy = f64::MAX;
        let mut mins: Vec<LocalPos<Pos2<usize>>> = Vec::new();

        for (pos, value) in self.0.iter()
            .filter_map(|(pos, value)|
            {
                if value.collapsed()
                {
                    return None;
                }

                edge_unmap(pos).map(|pos| (pos, value))
            })
        {
            let entropy = value.entropy();

            if entropy < lowest_entropy
            {
                lowest_entropy = entropy;

                mins.clear();
                mins.push(pos);
            } else if entropy == lowest_entropy
            {
                mins.push(pos);
            }
        }

        mins
    }

    fn lowest_entropy(&mut self, rng: &mut SeededRandom) -> Option<(LocalPos<Pos2<usize>>, &mut PossibleStates)>
    {
        let mins = self.lowest_entropies();

        if mins.is_empty()
        {
            None
        } else
        {
            let r = rng.next_usize_between(0..mins.len());

            let pos = mins[r];

            Some((pos, self.get_mut(edge_map(pos.into()))))
        }
    }
}

impl Index<CheckedPos<Pos2<usize>>> for Entropies
{
    type Output = PossibleStates;

    fn index(&self, index: CheckedPos<Pos2<usize>>) -> &Self::Output
    {
        self.get(index)
    }
}

impl IndexMut<CheckedPos<Pos2<usize>>> for Entropies
{
    fn index_mut(&mut self, index: CheckedPos<Pos2<usize>>) -> &mut Self::Output
    {
        self.get_mut(index)
    }
}

#[derive(Debug, Clone, Copy)]
struct VisitInfo
{
    pos: Pos2<usize>,
    states: usize
}

#[derive(Debug)]
struct VisitedCell
{
    a: VisitInfo,
    b: VisitInfo
}

type VisitedTracker = Vec<VisitedCell>;

struct WaveCollapserState
{
    rng: SeededRandom,
    entropies: Entropies,
    world_chunks: FlatChunksContainer<Option<WorldChunk>>
}

pub struct WaveCollapser<'a, 'b>
{
    rng: SeededRandom,
    rules: &'a ChunkRules,
    entropies: Entropies,
    world_chunks: &'b mut FlatChunksContainer<Option<WorldChunk>>,
    #[cfg(debug_assertions)]
    verbose_constrain: bool
}

impl Debug for WaveCollapser<'_, '_>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        f.debug_struct("WaveCollapser")
            .field("rng", &self.rng)
            .field("entropies", &self.entropies)
            .field("world_chunks", &self.world_chunks)
            .finish()
    }
}

impl<'a, 'm> WaveCollapser<'a, 'm>
{
    pub fn new<M: OvermapIndexing<OvermapDimension2>>(
        rng: SeededRandom,
        rules: &'a ChunkRules,
        world_chunks: &'m mut FlatChunksContainer<Option<WorldChunk>>,
        global_mapper: &M,
        mut edge_chunk: impl FnMut(GlobalPos<Pos2<i32>>) -> Option<WorldChunkId>
    ) -> Self
    {
        let entropies_size = world_chunks.size() + Pos2::repeat(ENTROPY_EDGE * 2);
        let entropies = Entropies(FlatChunksContainer::new_with(entropies_size, |pos|
        {
            let shifted_pos = pos.pos.map(|x| x as i32 - ENTROPY_EDGE as i32);

            let global_pos = global_mapper.to_global_unconverted(GlobalPos(shifted_pos));

            let new_uncollapsed = ||
            {
                PossibleStates::new(rules, chunk_difficulty(global_pos))
            };

            let size = world_chunks.size();

            if !(0..size.x as i32).contains(&shifted_pos.x) || !(0..size.y as i32).contains(&shifted_pos.y)
            {
                let states = if let Some(id) = edge_chunk(global_pos)
                {
                    PossibleStates::new_collapsed(id)
                } else
                {
                    new_uncollapsed()
                };

                return states;
            }

            if let Some(chunk) = world_chunks.get(shifted_pos.map(|x| x as usize)).unwrap()
            {
                if DebugConfig::is_enabled(DebugTool::RedundantWorldChecks)
                {
                    if let Some(known_chunk) = edge_chunk(global_pos)
                    {
                        debug_assert!(chunk.id() == known_chunk);
                    }
                }

                PossibleStates::new_collapsed(chunk.id())
            } else
            {
                new_uncollapsed()
            }
        }));

        let mut this = Self{
            rng,
            rules,
            entropies,
            world_chunks,
            #[cfg(debug_assertions)]
            verbose_constrain: DebugConfig::is_enabled(DebugTool::VerboseConstrain)
        };

        if DebugConfig::is_enabled(DebugTool::RedundantWorldChecks)
        {
            this.verify_states(global_mapper, &mut edge_chunk, false);
        }

        this.constrain_all();

        if DebugConfig::is_enabled(DebugTool::RedundantWorldChecks)
        {
            this.verify_states(global_mapper, &mut edge_chunk, true);
        }

        this
    }

    pub fn new_raw(
        rng: SeededRandom,
        rules: &'a ChunkRules,
        entropies: Entropies,
        world_chunks: &'m mut FlatChunksContainer<Option<WorldChunk>>
    ) -> Self
    {
        Self{
            rng,
            rules,
            entropies,
            world_chunks,
            #[cfg(debug_assertions)]
            verbose_constrain: false
        }
    }

    fn with_cloned<T>(&self, f: impl for<'b> FnOnce(WaveCollapser<'a, 'b>) -> T) -> T
    {
        let mut world_chunks = self.world_chunks.clone();

        f(WaveCollapser{
            rng: self.rng.clone(),
            rules: self.rules,
            entropies: self.entropies.clone(),
            world_chunks: &mut world_chunks,
            #[cfg(debug_assertions)]
            verbose_constrain: self.verbose_constrain
        })
    }

    fn restorable_state(&self) -> WaveCollapserState
    {
        WaveCollapserState{
            rng: self.rng.clone(),
            entropies: self.entropies.clone(),
            world_chunks: self.world_chunks.clone()
        }
    }

    fn restore(&mut self, state: WaveCollapserState)
    {
        self.rng = state.rng;
        self.entropies = state.entropies;
        *self.world_chunks = state.world_chunks;
    }

    #[cfg(debug_assertions)]
    pub fn set_verbose_constrain(&mut self, value: bool)
    {
        self.verbose_constrain = value;
    }

    pub fn entropies(&self) -> &Entropies
    {
        &self.entropies
    }

    fn verify_states(
        &self,
        global_mapper: &impl OvermapIndexing<OvermapDimension2>,
        mut edge_chunk: impl FnMut(GlobalPos<Pos2<i32>>) -> Option<WorldChunkId>,
        past_constrain: bool
    )
    {
        let edge_to_global_pos = |edge_pos: LocalPos<Pos2<usize>>| -> GlobalPos<Pos2<i32>>
        {
            global_mapper.to_global_unconverted(GlobalPos(edge_pos.pos.map(|x| x as i32 - ENTROPY_EDGE as i32)))
        };

        let mut verify_position_match = |edge_pos: LocalPos<Pos2<usize>>|
        {
            if let Some(loaded_id) = edge_chunk(edge_to_global_pos(edge_pos))
            {
                let current_states = &self.entropies[edge_pos.into()];

                assert!(current_states.collapsed());
                assert!(current_states.states().len() == 1);

                let first_state = current_states.states().first().unwrap();

                assert_eq!(loaded_id, first_state.id);
            }
        };

        for pos in self.entropies.positions()
        {
            verify_position_match(pos);

            pos.directions_group().map(|direction, value|
            {
                if let Some(direction_pos) = value
                {
                    verify_position_match(pos);
                    verify_position_match(direction_pos);

                    let first = &self.entropies[pos.into()];
                    let other = &self.entropies[direction_pos.into()];

                    if first.states().len() != 1
                    {
                        return;
                    }

                    let first = first.states().first().unwrap();

                    let direction = direction.flip_y();

                    let first_neighbors = self.rules.get(first.id).neighbors(direction);

                    assert!(other.states().len() > 0);

                    if !past_constrain && !other.collapsed()
                    {
                        return;
                    }

                    other.states().iter().for_each(|other_state|
                    {
                        if !first_neighbors.contains(&other_state.id)
                        {
                            eprintln!(
                                "{:?} ({:?}) {} doesnt have {:?} ({:?}) {} as valid a neighbor at {direction} (valid [{}])",
                                pos,
                                edge_to_global_pos(pos),
                                self.rules.format_id(first.id),
                                direction_pos,
                                edge_to_global_pos(direction_pos),
                                self.rules.format_id(other_state.id),
                                first_neighbors.iter().map(|x| self.rules.format_id(*x)).reduce(|acc, x| acc + ", " + &x).unwrap()
                            );

                            log_worldchunks("validity check failed:", self.world_chunks, chunks_edge_logger(&self.entropies));

                            panic!();
                        }
                    });
                }
            });
        }
    }

    fn constrain_all(&mut self)
    {
        for pos in self.entropies.positions()
        {
            let mut visited = Vec::new();

            self.constrain(&mut visited, pos, true);
        }
    }

    fn constrain(&mut self, visited: &mut VisitedTracker, pos: LocalPos<Pos2<usize>>, allow_fallback: bool) -> bool
    {
        let fmt_2d = |p: Pos2<usize>| -> String
        {
            format!("{}", p.map(|x| x as i32 - ENTROPY_EDGE as i32))
        };

        pos.directions_group().try_map(|direction, value|
        {
            if let Some(direction_pos) = value
            {
                let (this, other) = self.entropies.get_two_mut(pos.into(), direction_pos.into());

                let direction = direction.flip_y();

                let visited_index = {
                    let visited_index = visited.iter().position(|VisitedCell{a, b}|
                    {
                        (a.pos == pos.pos && b.pos == direction_pos.pos) || (b.pos == pos.pos && a.pos == direction_pos.pos)
                    });

                    if let Some(visited_index) = visited_index
                    {
                        let VisitedCell{a, b} = &mut visited[visited_index];

                        if a.pos == direction_pos.pos
                        {
                            std::mem::swap(a, b);
                        }

                        if a.states == this.states().len() && b.states == other.states().len()
                        {
                            return Some(());
                        }
                    }

                    visited_index
                };

                #[cfg(debug_assertions)]
                {
                    if self.verbose_constrain
                    {
                        eprintln!(
                            "{} constraining {} against {} ({} x {})",
                            direction.opposite(),
                            fmt_2d(direction_pos.pos),
                            fmt_2d(pos.pos),
                            other.format_states(self.rules),
                            this.format_states(self.rules)
                        );
                    }
                }

                let changed = other.constrain_both(self.rules, this, direction);

                {
                    let a_states = this.states().len();
                    let b_states = other.states().len();

                    if let Some(visited_index) = visited_index
                    {
                        let cell = &mut visited[visited_index];

                        cell.a.states = a_states;
                        cell.b.states = b_states;
                    } else
                    {
                        visited.push(VisitedCell{
                            a: VisitInfo{pos: pos.pos, states: a_states},
                            b: VisitInfo{pos: direction_pos.pos, states: b_states}
                        });
                    }
                }

                #[cfg(debug_assertions)]
                {
                    if self.verbose_constrain
                    {
                        eprintln!("after: {} x {}", other.format_states(self.rules), this.format_states(self.rules));
                    }
                }

                if let Err(is_second) = changed
                {
                    if !allow_fallback
                    {
                        return None;
                    }

                    let fallback_complain = |this_pos: LocalPos<Pos2<usize>>|
                    {
                        let neighbors = this_pos.directions_group().map(|direction, x|
                        {
                            x.map(|x|
                            {
                                let states = self.entropies.get(x.into()).format_states(self.rules);

                                format!("{}: {states}", direction.flip_y())
                            })
                        }).filter_map(convert::identity).into_iter().reduce(|acc, x|
                        {
                            acc + ", " + &x
                        }).unwrap_or_default();

                        eprintln!("couldnt find a valid worldchunk at {} with {neighbors}, using fallback", fmt_2d(this_pos.pos));
                    };

                    fallback_complain(if is_second { pos } else { direction_pos });

                    log_worldchunks("invalid stage", self.world_chunks, chunks_edge_logger(&self.entropies));

                    #[cfg(test)]
                    {
                        panic!();
                    }
                }

                let (other_changed, this_changed) = changed.unwrap_or((true, true));

                if this_changed
                {
                    self.constrain(visited, pos, allow_fallback);
                }

                if other_changed
                {
                    self.constrain(visited, direction_pos, allow_fallback);
                }
            }

            Some(())
        }).is_some()
    }

    pub fn lowest_entropy_with(&mut self, rng: &mut SeededRandom) -> Option<(LocalPos<Pos2<usize>>, &mut PossibleStates)>
    {
        self.entropies.lowest_entropy(rng)
    }

    pub fn generate_single_maybe<C>(
        &mut self,
        local: LocalPos<Pos2<usize>>,
        chunk: C
    )
    where
        C: FnOnce() -> WorldChunk
    {
        if self.world_chunks[local].is_none()
        {
            self.generate_single(local, chunk(), true);
        }
    }

    pub fn generate_single(
        &mut self,
        local: LocalPos<Pos2<usize>>,
        chunk: WorldChunk,
        allow_fallback: bool
    ) -> bool
    {
        let entropy_local = edge_map_local(local);

        self.entropies[entropy_local.into()].set_collapsed_id(chunk.id());

        self.world_chunks[local] = Some(chunk);

        let mut visited = Vec::new();
        self.constrain(&mut visited, entropy_local, allow_fallback)
    }

    pub fn generate_once(&mut self, rng: &mut SeededRandom, allow_fallback: bool) -> Option<bool>
    {
        if let Some((local_pos, state)) = self.entropies.lowest_entropy(rng)
        {
            let generated_chunk = self.rules.generate(state.collapse(self.rules, rng));

            let is_success = self.generate_single(local_pos, generated_chunk, allow_fallback);

            is_success.then_some(true)
        } else
        {
            Some(false)
        }
    }

    pub fn generate_once_force(&mut self, rng: &mut SeededRandom) -> bool
    {
        self.generate_once(rng, true).expect("with fallback it must always succeed")
    }

    fn generate_inner(&mut self, rng: &mut SeededRandom, allow_fallback: bool) -> bool
    {
        loop
        {
            match self.generate_once(rng, allow_fallback)
            {
                None => return false,
                Some(false) => return true,
                Some(true) => ()
            }
        }
    }

    pub fn generate(&mut self, allow_fallback: bool) -> bool
    {
        let mut rng = self.rng.clone();

        let output = self.generate_inner(&mut rng, allow_fallback);

        self.rng = rng;

        output
    }
}

#[cfg(test)]
mod tests
{
    use crate::common::{tilemap::TileLoot, world::TileRotation};

    use super::*;


    #[test]
    fn generating()
    {
        let tilemap = TileMap::parse(
            TileLoot{
                client: &mut Vec::new()
            },
            "info/tiles.json",
            "textures/tiles/"
        ).unwrap().tilemap;

        let get_tile = |name|
        {
            tilemap.tile_named(name).unwrap()
        };

        let a = get_tile("grassie");
        let b = get_tile("soil");
        let c = get_tile("glass");
        let d = get_tile("concrete");

        let parent_path = PathBuf::from("world_generation");
        let mut rules = ChunkRulesGroup::load(parent_path.clone()).unwrap();
        rules.insert_chunk("test_chunk".to_owned());

        let this_chunk_name = "test_chunk".to_owned();
        let this_chunk = rules.name_mappings().id_by_rotation_name(TileRotation::Up, this_chunk_name.clone()).unwrap();

        let rules = Rc::new(rules);

        let mut generator = ChunkGenerator::new(Rc::new(tilemap), rules, parent_path).unwrap();

        let info = ConditionalInfo{
            position: LocalPos::new(Pos2::repeat(0), Pos2::repeat(0)),
            height: 0,
            difficulty: 0.0
        };

        let tiles = generator.generate_chunk(&info, this_chunk, &this_chunk_name, 0, &mut |_| {});

        let check_tiles = ChunksContainer::from_raw(Pos3::new(8, 8, 1), Box::new([
            a,a,a,a,b,b,b,b,
            a,a,a,a,b,b,b,b,
            a,a,a,a,b,b,b,b,
            a,a,a,a,b,b,b,b,
            b,b,b,b,d,d,b,b,
            b,b,b,c,c,c,c,b,
            b,b,b,b,d,d,b,b,
            b,b,b,b,b,b,b,b,
        ]));

        let display_tiles = |tiles: &ChunksContainer<Tile>|
        {
            tiles.map(|&id|
            {
                if id == a
                {
                    "🥬"
                } else if id == b
                {
                    "🟫"
                } else if id == c
                {
                    "🥛"
                } else if id == d
                {
                    "🪨"
                } else
                {
                    panic!("id must be either a b c or d, got {id:?}, expected {a:?} {b:?} {c:?} {d:?}")
                }
            }).display()
        };

        if tiles != check_tiles
        {
            panic!(
                "grassie: {a:?}, soil: {b:?}, glass: {c:?}, concrete: {d:?}\n{:#?} != {:#?}",
                display_tiles(&tiles),
                display_tiles(&check_tiles)
            );
        }
    }
}
