use std::{
	io,
    fs,
	fmt,
    ops::Index,
    collections::HashSet,
	path::{Path, PathBuf}
};

use strum::IntoEnumIterator;

use parking_lot::Mutex;

use rlua::Lua;

use serde::{Serialize, Deserialize};

use crate::common::{
	TileMap,
    Saver,
    Saveable,
	world::{
		Pos3,
		LocalPos,
		AlwaysGroup,
		overmap::{OvermapIndexing, FlatChunksContainer, ChunksContainer, ChunkIndexing},
		chunk::{
            PosDirection,
            tile::Tile
        }
	}
};

use chunk_rules::ChunkRules;

mod chunk_rules;


#[derive(Debug)]
pub enum ParseErrorKind
{
	Io(io::Error),
	Json(serde_json::Error),
    Lua(rlua::Error)
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

impl From<rlua::Error> for ParseErrorKind
{
	fn from(value: rlua::Error) -> Self
	{
		ParseErrorKind::Lua(value)
	}
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ParseError
{
    filename: Option<PathBuf>,
    kind: ParseErrorKind
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

    pub fn printable(&self) -> Option<String>
    {
        match &self.kind
        {
            ParseErrorKind::Lua(lua) =>
            {
                match lua
                {
                    rlua::Error::SyntaxError{message, ..} =>
                    {
                        return Some(message.clone());
                    },
                    _ => ()
                }
            },
            _ => ()
        }

        None
    }
}

impl From<io::Error> for ParseError
{
    fn from(value: io::Error) -> Self
    {
        ParseError::new(value)
    }
}

impl From<rlua::Error> for ParseError
{
    fn from(value: rlua::Error) -> Self
    {
        ParseError::new(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WorldChunk
{
	id: usize
}

impl WorldChunk
{
	pub fn new(id: usize) -> Self
	{
		Self{id}
	}

	#[allow(dead_code)]
	pub fn none() -> Self
	{
		Self{id: 0}
	}

    pub fn is_none(&self) -> bool
    {
        self.id == 0
    }

	pub fn id(&self) -> usize
	{
		self.id
	}
}

impl Saveable for WorldChunk {}

pub const WORLD_CHUNK_SIZE: Pos3<usize> = Pos3{x: 16, y: 16, z: 1};

pub struct ChunkGenerator
{
    lua: Mutex<Lua>,
	tilemap: TileMap
}

impl ChunkGenerator
{
	pub fn new(tilemap: TileMap, rules: &ChunkRules) -> Result<Self, ParseError>
	{
		let lua = Mutex::new(Lua::new());

        let parent_directory = PathBuf::from("world_generation/chunks/");

        let mut this = Self{lua, tilemap};

        this.setup_lua_state()?;

		rules.iter().map(|rule|
		{
            let name = rule.name();
            let filename = parent_directory.join(format!("{name}.lua"));

			this.parse_function(filename, name)
		}).collect::<Result<(), _>>()?;

		Ok(this)
	}

    fn setup_lua_state(&mut self) -> Result<(), ParseError>
    {
        self.lua.lock().context(|ctx|
        {
            let tilemap = self.tilemap.names_map();

            ctx.globals().set("tilemap", tilemap)
        })?;

        Ok(())
    }

	fn parse_function(
        &mut self,
        filepath: PathBuf,
        function_name: &str
    ) -> Result<(), ParseError>
	{
        let code = fs::read_to_string(&filepath).map_err(|err|
        {
            // cant remove the clone cuz ? is cringe or something
            ParseError::new_named(filepath.clone(), err)
        })?;

        self.lua.lock().context(|ctx|
        {
            let function: rlua::Function = ctx.load(&code).set_name(function_name)?.eval()?;

            ctx.globals().set(function_name, function)?;

            Ok(())
        }).map_err(|err: rlua::Error| ParseError::new_named(filepath, err))?;
	    
        Ok(())
    }

	pub fn generate_chunk<'a>(
		&self,
		group: AlwaysGroup<&'a str>
	) -> ChunksContainer<Tile>
	{
        if group.this == "none"
        {
            return ChunksContainer::new_with(WORLD_CHUNK_SIZE, |_| Tile::none());
        }

        let tiles: Vec<Tile> = self.lua.lock().context(|ctx|
        {
            let function = ctx.globals().get::<_, rlua::Function>(group.this)?;

            let neighbors = PosDirection::iter().map(|direction| group[direction])
                .collect::<Vec<_>>();

            function.call(neighbors)
        }).expect("if this crashes its my bad lua skills anyways");

        ChunksContainer::new_indexed(WORLD_CHUNK_SIZE, |index| tiles[index])
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
	rules: ChunkRules
}

impl<S: Saver<WorldChunk>> WorldGenerator<S>
{
	pub fn new<P: AsRef<Path>>(
		saver: S,
		tilemap: TileMap,
		path: P
	) -> Result<Self, ParseError>
	{
        let rules = ChunkRules::load(path.as_ref())?;

		let generator = ChunkGenerator::new(tilemap, &rules)?;

		Ok(Self{generator, saver, rules})
	}

	pub fn generate_missing(
		&mut self,
		world_chunks: &mut ChunksContainer<Option<WorldChunk>>,
		global_mapper: &impl OvermapIndexing
	)
	{
        self.load_missing(world_chunks, global_mapper);

        for z in 0..world_chunks.size().z
        {
            let global_z = global_mapper.to_global_z(z);

            if global_z == 0
            {
                let mut wave_collapser = WaveCollapser::new(&self.rules, world_chunks, z);

                wave_collapser.generate(|local_pos, chunk|
                {
                    self.saver.save(global_mapper.to_global(local_pos), chunk);
                });
            } else
            {
                let this_slice = world_chunks.flat_slice_iter_mut(z).filter(|(_pos, chunk)|
                {
                    chunk.is_none()
                });

                if global_z > 0
                {
                    // above ground
                    
                    this_slice.for_each(|(local_pos, world_chunk)|
                    {
                        let chunk = WorldChunk::none();

                        self.saver.save(global_mapper.to_global(local_pos), chunk);

                        *world_chunk = Some(chunk);
                    });
                } else
                {
                    // underground

                    this_slice.for_each(|(local_pos, world_chunk)|
                    {
                        let chunk = WorldChunk::none();

                        self.saver.save(global_mapper.to_global(local_pos), chunk);

                        *world_chunk = Some(chunk);
                    });
                }
            }
        }
	}

	fn load_missing(
		&mut self,
		world_chunks: &mut ChunksContainer<Option<WorldChunk>>,
		global_mapper: &impl OvermapIndexing
	)
	{
		world_chunks.iter_mut()
            .filter(|(_pos, chunk)| chunk.is_none())
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
		&self,
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
    states: Vec<usize>,
    total: f64,
    entropy: f64,
    collapsed: bool,
    is_all: bool
}

impl PossibleStates
{
    pub fn new(rules: &ChunkRules) -> Self
    {
        let states = (1..rules.len()).collect();

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
        let other_states: &[usize] = if other.states.is_empty()
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

    pub fn collapse(&mut self, rules: &ChunkRules) -> usize
    {
        let id = if self.states.is_empty()
        {
            rules.fallback()
        } else if self.collapsed() || (self.states.len() == 1)
        {
            self.states[0]
        } else
        {
            let mut r = fastrand::f64() * self.total;

            *self.states.iter().find(|value|
            {
                let rule = rules.get(**value);

                r -= rule.weight();

                r <= 0.0
            }).expect("rules cannot be empty and all scaled weights must add up to 1")
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
    pub fn len(&self) -> usize
    {
        self.0.len()
    }

    pub fn index_to_pos(&self, index: usize) -> LocalPos
    {
        self.0.index_to_pos(index)
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
    world_chunks: &'a mut ChunksContainer<Option<WorldChunk>>,
    z_level: usize
}

impl<'a> WaveCollapser<'a>
{
    pub fn new(
        rules: &'a ChunkRules,
        world_chunks: &'a mut ChunksContainer<Option<WorldChunk>>,
        z_level: usize
    ) -> Self
    {
        let entropies = Entropies(world_chunks.map_slice_ref(z_level, |(_local_pos, chunk)|
        {
            if let Some(chunk) = chunk
            {
                PossibleStates::new_collapsed(chunk)
            } else
            {
                PossibleStates::new(rules)
            }
        }));

        let mut this = Self{rules, entropies, world_chunks, z_level};

        this.constrain_all();

        this
    }

    fn constrain_all(&mut self)
    {
        for i in 0..self.entropies.len()
        {
            let pos = self.entropies.index_to_pos(i);

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

                    let changed = other.constrain(&self.rules, this, direction);

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
        F: FnMut(LocalPos, WorldChunk)
    {
        let z_level = self.z_level;

		while let Some((local_pos, state)) = self.entropies.lowest_entropy()
		{
            let local_pos = LocalPos{
                pos: Pos3{
                    z: z_level,
                    ..local_pos.pos
                },
                size: local_pos.size
            };

            let generated_chunk = WorldChunk::new(state.collapse(&self.rules));

            on_chunk(local_pos, generated_chunk);

            self.world_chunks[local_pos] = Some(generated_chunk);

            let mut visited = VisitedTracker::new();
            self.constrain(&mut visited, local_pos);
		}
    }
}
