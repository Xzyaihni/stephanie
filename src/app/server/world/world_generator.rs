use std::{
    fs,
    io,
    fmt,
    sync::Arc,
    ops::Index,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf}
};

use parking_lot::Mutex;

use crate::common::{
    TileMap,
    SaveLoad,
    WeightedPicker,
    lisp::{
        self,
        Lisp,
        LispRef,
        LispConfig,
        LispMemory,
        Environment,
        Lambdas,
        LispValue,
        ValueTag,
        Primitives,
        PrimitiveProcedureInfo
    },
    world::{
        Pos3,
        LocalPos,
        AlwaysGroup,
        overmap::{Overmap, OvermapIndexing, FlatChunksContainer, ChunksContainer},
        chunk::{
            PosDirection,
            tile::Tile
        }
    }
};

use super::server_overmap::WorldPlane;

use chunk_rules::{ChunkRulesGroup, ChunkRules, ConditionalInfo};

pub use chunk_rules::{
    WORLD_CHUNK_SIZE,
    CHUNK_RATIO,
    MaybeWorldChunk,
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
    Lisp(lisp::Error)
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

impl From<lisp::Error> for ParseErrorKind
{
    fn from(value: lisp::Error) -> Self
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

impl From<lisp::Error> for ParseError
{
    fn from(value: lisp::Error) -> Self
    {
        ParseError::new(value)
    }
}

pub struct ChunkGenerator
{
    environment: Arc<Mutex<Environment<'static>>>,
    lambdas: Lambdas,
    primitives: Arc<Primitives>,
    memory: Arc<Mutex<LispMemory>>,
    chunks: HashMap<String, LispRef>,
    tilemap: TileMap
}

impl ChunkGenerator
{
    pub fn new(tilemap: TileMap, rules: &ChunkRulesGroup) -> Result<Self, ParseError>
    {
        let chunks = HashMap::new();

        let parent_directory = PathBuf::from("world_generation");

        let (environment, lambdas) = Self::default_environment(&parent_directory);

        let environment = Arc::new(Mutex::new(environment));
        let memory = Arc::new(Mutex::new(LispMemory::new(1024)));

        let primitives = Arc::new(Self::default_primitives(&tilemap));

        let mut this = Self{environment, lambdas, primitives, memory, chunks, tilemap};

        let parent_directory = parent_directory.join("chunks");

        rules.iter_names().filter(|name|
        {
            let name: &str = name.as_ref();

            name != "none"
        }).try_for_each(|name|
        {
            let filename = parent_directory.join(format!("{name}.scm"));

            this.parse_function(filename, name)
        })?;

        Ok(this)
    }

    fn default_primitives(tilemap: &TileMap) -> Primitives
    {
        let mut primitives = Primitives::new();

        let names_map: HashMap<String, Tile> = tilemap.names_owned_map();

        primitives.add(
            "tile",
            PrimitiveProcedureInfo::new_simple(1, move |_state, memory, _env, mut args|
            {
                let arg = args.pop(memory);

                let name = arg.as_symbol(memory)?;

                let tile = names_map[&name];

                memory.push_return(LispValue::new_integer(tile.id() as i32));

                Ok(())
            }));

        primitives
    }

    fn default_environment(path: &Path) -> (Environment<'static>, Lambdas)
    {
        let name = "default.scm";
        let path = path.join(name);
        let default_code = fs::read_to_string(path)
            .unwrap_or_else(|err| panic!("{name} must exist >_< ({err})"));

        let mut lisp = Lisp::new(&default_code)
            .unwrap_or_else(|err| panic!("{name} must be valid ({err})"));

        let env = lisp.run_environment()
            .unwrap_or_else(|err| panic!("{name} must run ({err})"));

        (env, lisp.lambdas().clone())
    }

    fn parse_function(
        &mut self,
        filepath: PathBuf,
        name: &str
    ) -> Result<(), ParseError>
    {
        let code = fs::read_to_string(&filepath).map_err(|err|
        {
            // cant remove the clone cuz ? is cringe or something
            ParseError::new_named(filepath.clone(), err)
        })?;

        let config = LispConfig{
            environment: Some(self.environment.clone()),
            lambdas: Some(self.lambdas.clone()),
            primitives: self.primitives.clone()
        };

        let lisp = unsafe{ LispRef::new_with_config(config, &code) }.unwrap_or_else(|err|
        {
            panic!("error parsing {name}: {err}")
        });

        self.chunks.insert(name.to_owned(), lisp);
        
        Ok(())
    }

    pub fn generate_chunk(
        &mut self,
        group: AlwaysGroup<&str>
    ) -> ChunksContainer<Tile>
    {
        if group.this == "none"
        {
            return ChunksContainer::new_with(WORLD_CHUNK_SIZE, |_| Tile::none());
        }

        let memory = &mut self.memory.lock();

        let chunk_name = group.this;
        let this_chunk = self.chunks.get_mut(chunk_name)
            .unwrap_or_else(||
            {
                panic!("worldchunk named `{}` doesnt exist", group.this)
            });

        let output = this_chunk.run_with_memory(memory)
            .unwrap_or_else(|err|
            {
                panic!("runtime lisp error: {err} (in {chunk_name})")
            });

        let output = output.as_vector_ref()
            .unwrap_or_else(|err|
            {
                panic!("expected vector: {err}")
            });

        if output.tag != ValueTag::Integer
        {
            panic!("wrong vector type `{:?}`", output.tag);
        }

        let tiles = output.values.iter().map(|x|
        {
            Tile::new(unsafe{ x.integer as usize })
        }).collect();

        ChunksContainer::from_raw(WORLD_CHUNK_SIZE, tiles)
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
    rules: ChunkRulesGroup
}

impl<S: SaveLoad<WorldChunk>> WorldGenerator<S>
{
    pub fn new(
        saver: S,
        tilemap: TileMap,
        path: impl Into<PathBuf>
    ) -> Result<Self, ParseError>
    {
        let rules = ChunkRulesGroup::load(path.into())?;

        let generator = ChunkGenerator::new(tilemap, &rules)?;

        Ok(Self{generator, saver, rules})
    }

    pub fn exit(&mut self)
    {
        self.saver.exit();
    }

    pub fn generate_surface(
        &mut self,
        world_chunks: &mut FlatChunksContainer<Option<WorldChunk>>,
        global_mapper: &impl OvermapIndexing
    )
    {
        self.load_missing(world_chunks.iter_mut(), global_mapper);

        let mut wave_collapser = WaveCollapser::new(&self.rules.surface, world_chunks);

        wave_collapser.generate(|local_pos, chunk|
        {
            self.saver.save(global_mapper.to_global(local_pos), chunk.clone());
        });
    }

    pub fn generate_missing(
        &mut self,
        world_chunks: &mut ChunksContainer<Option<WorldChunk>>,
        world_plane: &WorldPlane<S>,
        global_mapper: &impl OvermapIndexing
    )
    {
        self.load_missing(world_chunks.iter_mut(), global_mapper);

        for z in (0..world_chunks.size().z).rev()
        {
            let global_z = global_mapper.to_global_z(z);

            if global_z == 0
            {
                continue;
            }

            let this_slice = world_chunks.flat_slice_iter_mut(z).filter(|(_pos, chunk)|
            {
                chunk.is_none()
            });

            let mut applier = |pair: (LocalPos, &mut Option<WorldChunk>), chunk: WorldChunk|
            {
                self.saver.save(global_mapper.to_global(pair.0), chunk.clone());

                *pair.1 = Some(chunk);
            };

            if global_z > 0
            {
                // above ground
                
                this_slice.for_each(|pair|
                {
                    let pos = pair.0;

                    let this_surface = world_plane.get_local(pos)
                        .as_ref()
                        .expect("world_plane must be completely generated");

                    let info = ConditionalInfo{
                        height: global_z,
                        tags: this_surface.tags()
                    };

                    let chunk = self.rules.city.generate(info);

                    applier(pair, chunk);
                });
            } else
            {
                // underground

                this_slice.for_each(|pair|
                {
                    let chunk = WorldChunk::new(self.rules.underground.fallback(), Vec::new());

                    applier(pair, chunk);
                });
            }
        }
    }

    pub fn rules(&self) -> &ChunkRulesGroup
    {
        &self.rules
    }

    fn load_missing<'a>(
        &mut self,
        world_chunks: impl Iterator<Item=(LocalPos, &'a mut Option<WorldChunk>)>,
        global_mapper: &impl OvermapIndexing
    )
    {
        world_chunks.filter(|(_pos, chunk)| chunk.is_none())
            .for_each(|(pos, chunk)|
            {
                let loaded_chunk = self.saver.load(global_mapper.to_global(pos));

                if loaded_chunk.is_some()
                {
                    *chunk = loaded_chunk;
                }
            });
    }

    pub fn generate_chunk(
        &mut self,
        group: AlwaysGroup<WorldChunk>
    ) -> ChunksContainer<Tile>
    {
        self.generator.generate_chunk(group.map(|world_chunk|
        {
            self.rules.name(world_chunk.id())
        }))
    }
}

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
        let states = rules.ids().copied().collect();

        Self{
            states,
            total: rules.total_weight(),
            entropy: rules.entropy(),
            collapsed: false,
            is_all: true
        }
    }

    pub fn new_collapsed(chunk: &WorldChunk) -> Self
    {
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

        self.states = vec![id];
        self.collapsed = true;
        self.is_all = false;

        id
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

                    let changed = other.constrain(self.rules, this, direction);

                    if changed
                    {
                        self.constrain(visited, direction_pos);
                    }
                }
            });
        }
    }

    pub fn generate<F>(&mut self, mut on_chunk: F)
    where
        F: FnMut(LocalPos, &WorldChunk)
    {
        while let Some((local_pos, state)) = self.entropies.lowest_entropy()
        {
            let generated_chunk = self.rules.generate(state.collapse(self.rules));

            on_chunk(local_pos, &generated_chunk);

            self.world_chunks[local_pos] = Some(generated_chunk);

            let mut visited = VisitedTracker::new();
            self.constrain(&mut visited, local_pos);
        }
    }
}
