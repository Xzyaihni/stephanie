use std::{
    fs,
    io,
    fmt::{self, Display, Debug},
    str::FromStr,
    rc::Rc,
    ops::{Index, IndexMut},
    cmp::Ordering,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf}
};

use nalgebra::Vector3;

use crate::common::{
    TileMap,
    SaveLoad,
    WeightedPicker,
    WorldChunksBlock,
    lisp::{self, *},
    world::{
        Pos3,
        LocalPos,
        GlobalPos,
        AlwaysGroup,
        Chunk,
        overmap::{
            CommonIndexing,
            OvermapIndexing,
            FlatIndexer,
            FlatChunksContainer,
            ChunksContainer
        },
        chunk::{
            PosDirection,
            tile::{Tile, TileRotation}
        }
    }
};

use super::{
    SERVER_OVERMAP_SIZE_Z,
    MarkerTile,
    MarkerKind,
    server_overmap::WorldPlane
};

use chunk_rules::ChunkRules;

pub use chunk_rules::{
    WORLD_CHUNK_SIZE,
    CHUNK_RATIO,
    ChunkRulesGroup,
    ConditionalInfo,
    WorldChunk,
    WorldChunkId,
    WorldChunkTag
};

mod chunk_rules;


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

pub fn chunk_difficulty(pos: GlobalPos) -> f32
{
    let mut p: Vector3<f32> = Vector3::from(pos.0).cast();
    p.z = 0.0;

    p.magnitude() * 0.01
}

pub enum ChunkGenerationError
{
    SymbolAllocation(String, lisp::Error),
    TagSymbolAllocation(lisp::Error),
    LispRuntime(lisp::ErrorPos),
    WrongOutput(lisp::Error),
    Tile(lisp::Error)
}

impl Display for ChunkGenerationError
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::SymbolAllocation(name, err) => write!(f, "error allocating {name} symbol: {err}"),
            Self::TagSymbolAllocation(err) => write!(f, "error allocating tag symbol: {err}"),
            Self::LispRuntime(err) => write!(f, "{err}"),
            Self::WrongOutput(err) => write!(f, "expected vector: {err}"),
            Self::Tile(err) => write!(f, "error getting tile: {err}")
        }
    }
}

pub struct ChunkGenerator
{
    rules: Rc<ChunkRulesGroup>,
    chunks: HashMap<String, Lisp>,
    tilemap: Rc<TileMap>
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

        let primitives = Rc::new(Self::default_primitives(&tilemap));

        let memory = LispMemory::new(primitives, 256, 1 << 13);

        let mut this = Self{
            rules: rules.clone(),
            chunks,
            tilemap
        };

        rules.iter_names().filter(|(rotation, name)|
        {
            let name: &str = name.as_ref();

            name != "none" && *rotation == TileRotation::Up
        }).try_for_each(|(_, name)|
        {
            this.parse_function(&parent_directory, memory.clone(), name)
        })?;

        Ok(this)
    }

    pub fn default_primitives(tilemap: &TileMap) -> Primitives
    {
        let mut primitives = Primitives::default();

        let fallback_tile = Tile::none();
        let names_map: HashMap<String, Tile> = tilemap.names_owned_map();

        primitives.add(
            "tile",
            PrimitiveProcedureInfo::new_simple(ArgsCount::Min(1), Effect::Pure, move |mut args|
            {
                let name = args.next().unwrap().as_symbol(args.memory)?;
                let rotation = args.next();

                let mut tile = *names_map.get(&name).unwrap_or_else(||
                {
                    eprintln!("no tile named `{name}`, using fallback");

                    &fallback_tile
                });

                if let Some(rotation) = rotation
                {
                    let name = rotation.as_symbol(args.memory)?;

                    match TileRotation::from_str(&name)
                    {
                        Ok(x) =>
                        {
                            if let Some(tile) = &mut tile.0
                            {
                                tile.set_rotation(x);
                            } else
                            {
                                eprintln!("air cannot have rotation");
                            }
                        },
                        Err(_) => eprintln!("no rotation named `{name}`")
                    }
                }

                tile.as_lisp_value(args.memory)
            }));

        primitives
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

        let filepath = parent_directory.join("chunks").join(format!("{name}.scm"));

        let standard_code = load("lisp/standard.scm");
        let default_code = load(parent_directory.join("default.scm"));
        let chunk_code = fs::read_to_string(&filepath).map_err(|err|
        {
            // cant remove the clone cuz ? is cringe or something
            ParseError::new_named(filepath.clone(), err)
        })?;

        let config = LispConfig{
            type_checks: cfg!(debug_assertions),
            memory
        };

        let lisp = Lisp::new_with_config(config, &[&standard_code, &default_code, &chunk_code]).map_err(|err|
        {
            ParseError::new_named(PathBuf::from(name), err)
        })?;

        self.chunks.insert(name.to_owned(), lisp);

        Ok(())
    }

    pub fn generate_chunk_with(
        info: &ConditionalInfo,
        rules: &ChunkRulesGroup,
        this_chunk: &mut Lisp,
        marker: &mut impl FnMut(MarkerTile)
    ) -> Result<ChunksContainer<Tile>, ChunkGenerationError>
    {
        let tiles = {
            let mut define_symbol = |name, value|
            {
                this_chunk.memory_mut().define(name, value).map_err(|err|
                {
                    ChunkGenerationError::SymbolAllocation(name.to_owned(), err)
                })
            };

            define_symbol("height", info.height.into())?;
            define_symbol("difficulty", info.difficulty.into())?;
            define_symbol("rotation", (info.rotation as i32).into())?;

            info.tags.iter().try_for_each(|tag|
            {
                tag.define(rules.name_mappings(), this_chunk.memory_mut())
            }).map_err(|err|
            {
                ChunkGenerationError::TagSymbolAllocation(err)
            })?;

            let (memory, value): (LispMemory, LispValue) = this_chunk.run()
                .map_err(|err|
                {
                    ChunkGenerationError::LispRuntime(err)
                })?
                .destructure();

            let output = value.as_vector_ref(&memory).map_err(|err|
            {
                ChunkGenerationError::WrongOutput(err)
            })?;

            debug_assert!(WORLD_CHUNK_SIZE.z == 1, "i didnt implement rotation for anything other than z 1");
            debug_assert!(WORLD_CHUNK_SIZE.x == WORLD_CHUNK_SIZE.y, "cant rotate non square chunks");
            assert_eq!(output.len(), WORLD_CHUNK_SIZE.product());

            fn process<'a>(
                memory: &LispMemory,
                marker: &mut impl FnMut(MarkerTile),
                rotation: TileRotation,
                values: impl Iterator<Item=&'a LispValue>
            ) -> Result<Box<[Tile]>, ChunkGenerationError>
            {
                values.enumerate().map(|(index, x)|
                {
                    let x = OutputWrapperRef::new(memory, *x);
                    if let Ok(s) = x.as_list().and_then(|lst| lst.car.as_symbol())
                    {
                        if s != "marker"
                        {
                            return Err(lisp::Error::Custom(format!("malformed tile, expected marker got {s}")));
                        }

                        let value = x.as_list().unwrap().cdr;

                        let pos = Chunk::index_to_pos(index);
                        MarkerKind::from_lisp_value(value)?.into_iter().for_each(|marker_tile|
                        {
                            marker(MarkerTile{kind: marker_tile.rotated(rotation), pos});
                        });

                        Ok(Tile::none())
                    } else
                    {
                        Tile::from_lisp_value(x)
                    }
                }).collect::<Result<Box<[Tile]>, _>>().map_err(|err|
                {
                    ChunkGenerationError::Tile(err)
                })
            }

            const SPAN: usize = WORLD_CHUNK_SIZE.x;
            let rotation = info.rotation;
            let values = match rotation
            {
                TileRotation::Up => process(&memory, marker, rotation, output.iter()),
                TileRotation::Right =>
                {
                    let values = (0..SPAN).flat_map(|x|
                    {
                        (0..SPAN).rev().map(move |y| y * SPAN + x)
                    }).map(|index| &output[index]);

                    process(&memory, marker, rotation, values)
                },
                TileRotation::Left =>
                {
                    let values = (0..SPAN).rev().flat_map(|x|
                    {
                        (0..SPAN).map(move |y| y * SPAN + x)
                    }).map(|index| &output[index]);

                    process(&memory, marker, rotation, values)
                },
                TileRotation::Down => process(&memory, marker, rotation, output.iter().rev())
            };

            values?
        };

        Ok(ChunksContainer::from_raw(WORLD_CHUNK_SIZE, tiles))
    }

    pub fn generate_chunk(
        &mut self,
        info: &ConditionalInfo,
        group: AlwaysGroup<&str>,
        marker: &mut impl FnMut(MarkerTile)
    ) -> ChunksContainer<Tile>
    {
        let chunk_name = group.this;
        let this_chunk = self.chunks.get_mut(chunk_name).unwrap_or_else(||
        {
            panic!("worldchunk named `{}` doesnt exist", group.this)
        });

        Self::generate_chunk_with(info, &self.rules, this_chunk, marker).unwrap_or_else(|err|
        {
            panic!("{err} in ({chunk_name})")
        })
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
    generator: ChunkGenerator,
    saver: S,
    rules: Rc<ChunkRulesGroup>
}

impl<S: SaveLoad<WorldChunksBlock>> WorldGenerator<S>
{
    pub fn new(
        saver: S,
        tilemap: Rc<TileMap>,
        path: impl Into<PathBuf>
    ) -> Result<Self, ParseError>
    {
        let path = path.into();
        let rules = Rc::new(ChunkRulesGroup::load(path.clone())?);

        let generator = ChunkGenerator::new(tilemap, rules.clone(), path)?;

        Ok(Self{generator, saver, rules})
    }

    pub fn generate_surface<M: OvermapIndexing + Debug>(
        &mut self,
        world_chunks: &mut FlatChunksContainer<Option<WorldChunksBlock>>,
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
                chunk_positions.iter().all(|pos| global_mapper.to_global(*pos).0.z == 0),
                "z must be 0, {global_mapper:#?} {chunk_positions:#?}"
            );
        }

        self.load_missing(world_chunks.iter_mut(), global_mapper);

        plane.0.iter_mut().zip(world_chunks.iter()).for_each(|((_, plane), (_, world))|
        {
            *plane = world.as_ref().map(|chunk| chunk[0].clone());
        });

        let mut wave_collapser = WaveCollapser::new(&self.rules.surface, &mut plane.0);

        if let Some(local) = global_mapper.to_local(GlobalPos::new(0, 0, 0))
        {
            wave_collapser.generate_single_maybe(
                LocalPos::new(Pos3{z: 0, ..local.pos}, Pos3{z: 1, ..local.size}),
                ||
                {
                    let random_rotation = TileRotation::random();
                    self.rules.name_mappings().world_chunk.get(&(random_rotation, "bunker".to_owned())).map(|bunker_id|
                    {
                        WorldChunk::new(*bunker_id, Vec::new())
                    }).unwrap_or_else(WorldChunk::none)
                }
            );
        }

        wave_collapser.generate();
    }

    pub fn generate_missing(
        &mut self,
        world_chunks: &mut ChunksContainer<Option<WorldChunksBlock>>,
        world_plane: &WorldPlane,
        global_mapper: &impl OvermapIndexing
    )
    {
        debug_assert!(world_plane.all_exist());
        debug_assert!(world_chunks.size().z == SERVER_OVERMAP_SIZE_Z);
        debug_assert!(global_mapper.size() == world_chunks.size());

        self.load_missing(world_chunks.iter_mut(), global_mapper);

        #[cfg(debug_assertions)]
        {
            use crate::debug_config::*;

            if DebugConfig::is_enabled(DebugTool::RedundantWorldChecks)
            {
                world_plane.0.iter().for_each(|(pos, value)|
                {
                    let global_pos = global_mapper.to_global(
                        LocalPos::new(pos.pos, Pos3{z: global_mapper.size().z, ..pos.size})
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
                let s = world_chunks.map_slice_ref(local_z, |(pos, x)|
                {
                    let plane_pos = LocalPos::new(Pos3{z: 0, ..pos.pos}, Pos3{z: 1, ..pos.size});
                    (world_plane.0[plane_pos].clone(), x.as_ref().map(|x| x[0].clone()))
                });

                debug_assert!(
                    s.iter().all(|(_, (a, b))| b.as_ref().map(|b| *a == Some(b.clone())).unwrap_or(true)),
                    "world plane must match the worldchunks: {s:#?}"
                );
            }
        }

        let indexer = FlatIndexer::new(world_chunks.size());
        (0..indexer.size().product()).for_each(|index|
        {
            let flat_local = indexer.index_to_pos(index);

            (0..SERVER_OVERMAP_SIZE_Z).rev().for_each(|z|
            {
                let size = world_chunks.size();

                let local_pos = LocalPos::new(Pos3{z, ..flat_local.pos}, size);

                let this_world_chunk = &mut world_chunks[local_pos];
                if this_world_chunk.is_some()
                {
                    return;
                }

                let global_pos = global_mapper.to_global(local_pos);

                let difficulty = chunk_difficulty(global_pos);

                let block: WorldChunksBlock = (0..CHUNK_RATIO.z).map(|index|
                {
                    let mut global_pos = global_pos;
                    global_pos.0.z = global_pos.0.z * CHUNK_RATIO.z as i32 + index as i32;

                    let global_z = global_pos.0.z;

                    let this_surface = world_plane.world_chunk(
                        LocalPos::new(Pos3{z: 0, ..local_pos.pos}, Pos3{z: 1, ..local_pos.size})
                    );

                    match global_z.cmp(&0)
                    {
                        Ordering::Equal => this_surface.clone(),
                        Ordering::Greater =>
                        {
                            // above ground

                            let rotation = self.rotation_of(this_surface.id());

                            let info = ConditionalInfo{
                                height: global_z,
                                difficulty,
                                rotation,
                                tags: this_surface.tags()
                            };

                            self.rules.city.generate(info, this_surface.id())
                        },
                        Ordering::Less =>
                        {
                            // underground

                            let rotation = self.rotation_of(this_surface.id());

                            let info = ConditionalInfo{
                                height: global_z,
                                difficulty,
                                rotation,
                                tags: this_surface.tags()
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
        group: AlwaysGroup<WorldChunk>,
        marker: &mut impl FnMut(MarkerTile)
    ) -> ChunksContainer<Tile>
    {
        if group.this.id() == WorldChunkId::none()
        {
            return ChunksContainer::new_with(WORLD_CHUNK_SIZE, |_| Tile::none());
        }

        self.generator.generate_chunk(info, group.map(|world_chunk|
        {
            self.rules.name(world_chunk.id())
        }), marker)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PossibleStates
{
    states: Vec<WorldChunkId>,
    total: f64,
    entropy: f64,
    collapsed: bool,
    is_all: bool
}

impl PossibleStates
{
    pub fn new(rules: &ChunkRules) -> Self
    {
        let states: Vec<_> = rules.ids().copied().collect();

        {
            let total_weight = states.iter().map(|x| rules.get(*x).weight()).sum::<f64>();
            debug_assert!((total_weight - 1.0).abs() < 0.0001, "{total_weight} must be 1.0");
        }

        Self{
            states,
            total: 1.0,
            entropy: rules.entropy(),
            collapsed: false,
            is_all: true
        }
    }

    pub fn new_collapsed(chunk: &WorldChunk) -> Self
    {
        debug_assert!(chunk.id() != WorldChunkId::none());

        Self{
            states: vec![chunk.id()],
            total: 1.0,
            entropy: 0.0,
            collapsed: true,
            is_all: false
        }
    }

    pub fn constrain(
        &mut self,
        rules: &ChunkRules,
        other: &PossibleStates,
        direction: PosDirection
    ) -> bool
    {
        if other.is_all() || self.collapsed()
        {
            return false;
        }

        let mut any_constrained = false;

        let fallback_array = [rules.fallback()];
        let other_states: &[WorldChunkId] = if other.states.is_empty()
        {
            eprintln!("using fallback worldchunk");
            &fallback_array
        } else
        {
            &other.states
        };

        self.states.retain(|state|
        {
            let keep = other_states.iter().any(|other_state|
            {
                let other_rule = rules.get(*other_state);
                let possible = &other_rule.neighbors(direction);

                possible.contains(state)
            });

            if !keep
            {
                let this_rule = rules.get(*state);

                self.total -= this_rule.weight();
                any_constrained = true;
            }

            keep
        });

        if any_constrained
        {
            self.is_all = false;
            self.update_entropy(rules);
        }

        any_constrained
    }

    pub fn collapse(&mut self, rules: &ChunkRules) -> WorldChunkId
    {
        let id = if self.states.is_empty()
        {
            rules.fallback()
        } else if self.collapsed() || (self.states.len() == 1)
        {
            self.states[0]
        } else
        {
            *WeightedPicker::new(self.total, &self.states)
                .pick_with(fastrand::f64(), |value|
                {
                    let rule = rules.get(*value);

                    rule.weight()
                })
                .expect("rules cannot be empty")
        };

        self.set_collapsed_id(id);

        id
    }

    pub fn set_collapsed_id(&mut self, id: WorldChunkId)
    {
        self.states = vec![id];
        self.collapsed = true;
        self.is_all = false;
    }

    fn update_entropy(&mut self, rules: &ChunkRules)
    {
        self.entropy = Self::calculate_entropy(self.states.iter().map(|state|
        {
            rules.get(*state).weight()
        }));
    }

    pub fn calculate_entropy(weights: impl Iterator<Item=f64>) -> f64
    {
        let s: f64 = weights.map(|value|
        {
            value * value.ln()
        }).sum();

        -s
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
    fn format_states(&self) -> String
    {
        let states = self.states.iter().map(|x| x.to_string()).reduce(|mut acc, x|
        {
            acc += ", ";
            acc += &x;
            acc
        }).unwrap_or_default();

        format!("[{states}]")
    }
}

// extremely useful struct (not)
// a vec would probably be faster cuz cpu caching and how much it allocates upfront and blablabla
struct VisitedTracker(HashSet<Pos3<usize>>);

impl VisitedTracker
{
    pub fn new() -> Self
    {
        Self(HashSet::new())
    }

    pub fn visit(&mut self, value: Pos3<usize>) -> bool
    {
        self.0.insert(value)
    }

    #[allow(dead_code)]
    pub fn visited(&self, value: &Pos3<usize>) -> bool
    {
        self.0.contains(value)
    }
}

struct Entropies(FlatChunksContainer<PossibleStates>);

impl Entropies
{
    pub fn positions(&self) -> impl Iterator<Item=LocalPos>
    {
        self.0.positions()
    }

    pub fn get_two_mut(
        &mut self,
        one: LocalPos,
        two: LocalPos
    ) -> (&mut PossibleStates, &mut PossibleStates)
    {
        self.0.get_two_mut(one, two)
    }

    pub fn lowest_entropy(&mut self) -> Option<(LocalPos, &mut PossibleStates)>
    {
        let mut lowest_entropy = f64::MAX;
        let mut mins: Vec<(LocalPos, &mut PossibleStates)> = Vec::new();

        for (pos, value) in self.0.iter_mut()
            .filter(|(_pos, value)| !value.collapsed())
        {
            let entropy = value.entropy();

            if entropy < lowest_entropy
            {
                lowest_entropy = entropy;

                mins.clear();
                mins.push((pos, value));
            } else if entropy == lowest_entropy
            {
                mins.push((pos, value));
            }
        }

        if mins.is_empty()
        {
            None
        } else
        {
            let r = fastrand::usize(0..mins.len());

            Some(mins.remove(r))
        }
    }
}

impl Index<LocalPos> for Entropies
{
    type Output = PossibleStates;

    fn index(&self, index: LocalPos) -> &Self::Output
    {
        &self.0[index]
    }
}

impl IndexMut<LocalPos> for Entropies
{
    fn index_mut(&mut self, index: LocalPos) -> &mut Self::Output
    {
        &mut self.0[index]
    }
}

struct WaveCollapser<'a>
{
    rules: &'a ChunkRules,
    entropies: Entropies,
    world_chunks: &'a mut FlatChunksContainer<Option<WorldChunk>>
}

impl<'a> WaveCollapser<'a>
{
    pub fn new(
        rules: &'a ChunkRules,
        world_chunks: &'a mut FlatChunksContainer<Option<WorldChunk>>
    ) -> Self
    {
        let entropies = Entropies(world_chunks.map(|chunk|
        {
            if let Some(chunk) = chunk
            {
                PossibleStates::new_collapsed(chunk)
            } else
            {
                PossibleStates::new(rules)
            }
        }));

        let mut this = Self{rules, entropies, world_chunks};

        this.constrain_all();

        this
    }

    fn constrain_all(&mut self)
    {
        for pos in self.entropies.positions()
        {
            let mut visited = VisitedTracker::new();
            self.constrain(&mut visited, pos);
        }
    }

    fn constrain(&mut self, visited: &mut VisitedTracker, pos: LocalPos)
    {
        if visited.visit(pos.pos)
        {
            pos.directions_group().map(|direction, value|
            {
                if let Some(direction_pos) = value
                {
                    let (this, other) = self.entropies.get_two_mut(pos, direction_pos);

                    let changed = other.constrain(self.rules, this, direction.flip_y());

                    if changed
                    {
                        self.constrain(visited, direction_pos);
                    }
                }
            });
        }
    }

    pub fn generate_single_maybe<C>(
        &mut self,
        local: LocalPos,
        chunk: C
    )
    where
        C: FnOnce() -> WorldChunk
    {
        if self.world_chunks[local].is_none()
        {
            self.generate_single(local, chunk());
        }
    }

    pub fn generate_single(
        &mut self,
        local: LocalPos,
        chunk: WorldChunk
    )
    {
        debug_assert!(local.pos.z == 0, "{local:#?}");

        self.entropies[local].set_collapsed_id(chunk.id());

        self.world_chunks[local] = Some(chunk);

        let mut visited = VisitedTracker::new();
        self.constrain(&mut visited, local);
    }

    pub fn generate(&mut self)
    {
        while let Some((local_pos, state)) = self.entropies.lowest_entropy()
        {
            let generated_chunk = self.rules.generate(state.collapse(self.rules));

            self.generate_single(local_pos, generated_chunk);
        }
    }
}

#[cfg(test)]
mod tests
{
    use crate::common::world::{DirectionsGroup, TileRotation};

    use super::*;


    #[test]
    fn generating()
    {
        let tilemap = TileMap::parse("tiles/tiles.json", "textures/tiles/").unwrap().tilemap;

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

        let rules = Rc::new(rules);

        let mut generator = ChunkGenerator::new(Rc::new(tilemap), rules, parent_path).unwrap();

        let empty = [];
        let info = ConditionalInfo{
            height: 0,
            difficulty: 0.0,
            rotation: TileRotation::Up,
            tags: &empty
        };

        let tiles = generator.generate_chunk(&info, AlwaysGroup{
            this: "test_chunk",
            other: DirectionsGroup{
                right: "none",
                left: "none",
                down: "none",
                up: "none"
            }
        }, &mut |_| {});

        let check_tiles = ChunksContainer::from_raw(Pos3::new(16, 16, 1), Box::new([
            a,a,a,a,b,b,b,b,c,c,d,d,d,d,d,d,
            a,a,a,a,b,b,b,b,b,b,d,d,d,d,d,d,
            a,a,a,a,b,b,b,b,b,b,b,b,b,b,b,b,
            a,a,a,a,b,b,b,b,b,b,b,b,b,b,b,b,
            b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,
            b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,
            b,b,b,b,b,b,b,c,c,c,c,b,b,b,b,b,
            b,b,b,b,b,b,b,c,c,c,c,b,b,b,b,b,
            b,b,b,b,b,b,b,c,c,c,c,b,b,b,b,b,
            b,b,b,d,d,d,d,d,d,d,d,d,b,b,b,b,
            b,b,b,d,d,d,d,d,d,d,d,d,b,b,b,b,
            b,b,b,d,d,d,d,d,d,d,d,d,b,b,b,b,
            b,b,b,d,d,d,d,d,d,d,d,d,b,b,b,b,
            b,b,b,b,b,b,b,c,c,c,c,b,b,b,b,b,
            b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,
            b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,b,
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
