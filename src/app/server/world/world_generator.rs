use std::{
	io,
    fs,
	fmt,
	fs::File,
	path::{Path, PathBuf}
};

use strum::IntoEnumIterator;

use parking_lot::Mutex;

use rlua::Lua;

use serde::{Serialize, Deserialize};

use super::ChunkSaver;

use crate::common::{
	TileMap,
	world::{
		Pos3,
		LocalPos,
		GlobalPos,
		AlwaysGroup,
		DirectionsGroup,
		overmap::ChunksContainer,
		chunk::{
            PosDirection,
            tile::Tile
        }
	}
};


#[derive(Debug, Serialize, Deserialize)]
pub struct NeighborChance
{
	pub weight: usize,
	pub name: String
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkRule
{
	pub name: String,
	pub neighbors: DirectionsGroup<Vec<NeighborChance>>
}

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

	pub fn id(&self) -> usize
	{
		self.id
	}
}

pub const WORLD_CHUNK_SIZE: Pos3<usize> = Pos3{x: 16, y: 16, z: 1};

pub struct ChunkGenerator
{
    lua: Mutex<Lua>,
	tilemap: TileMap
}

impl ChunkGenerator
{
	pub fn new(tilemap: TileMap, chunk_rules: &[ChunkRule]) -> Result<Self, ParseError>
	{
		let lua = Mutex::new(Lua::new());

        let parent_directory = "world_generation/chunks/";

        let mut this = Self{lua, tilemap};

        this.setup_lua_state()?;

		chunk_rules.iter().filter(|rule| rule.name != "none").map(|rule|
		{
            let filename = format!("{parent_directory}{}.lua", rule.name);

			this.parse_function(filename, &rule.name)
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
        filename: String,
        function_name: &str
    ) -> Result<(), ParseError>
	{
        let filepath = PathBuf::from(&filename);

        let code = fs::read_to_string(filename).map_err(|err|
        {
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
            return ChunksContainer::new(WORLD_CHUNK_SIZE, |_| Tile::none());
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
pub struct WorldGenerator
{
	chunk_generator: ChunkGenerator,
	chunk_saver: ChunkSaver<WorldChunk>,
	chunk_rules: Box<[ChunkRule]>
}

impl WorldGenerator
{
	pub fn new<P: AsRef<Path>>(
		chunk_saver: ChunkSaver<WorldChunk>,
		tilemap: TileMap,
		path: P
	) -> Result<Self, ParseError>
	{
        let json_file = File::open(&path).map_err(|err|
        {
            ParseError::new_named(path.as_ref().to_owned(), err)
        })?;

		let chunk_rules = serde_json::from_reader::<_, Box<[ChunkRule]>>(json_file).map_err(|err|
        {
            ParseError::new_named(path.as_ref().to_owned(), err)
        })?;

		let chunk_generator = ChunkGenerator::new(tilemap, &chunk_rules)?;

		Ok(Self{chunk_generator, chunk_saver, chunk_rules})
	}

	pub fn generate_missing<F>(
		&self,
		world_chunks: &mut ChunksContainer<Option<WorldChunk>>,
		mut to_global: F
	)
	where
		F: FnMut(LocalPos) -> GlobalPos
	{
		self.load_missing(world_chunks, &mut to_global);
		self.generate_wave_collapse(world_chunks, to_global);
	}

	fn generate_wave_collapse<F>(
		&self,
		world_chunks: &mut ChunksContainer<Option<WorldChunk>>,
		mut to_global: F
	)
	where
		F: FnMut(LocalPos) -> GlobalPos
	{
		/*loop
		{
			break;
		}*/
		world_chunks.iter_mut().filter(|(_, chunk)| chunk.is_none())
			.for_each(|(pos, chunk)|
			{
                let pos = to_global(pos);
				let generated_chunk = self.wave_collapse(pos);

				*chunk = Some(generated_chunk);
			});
	}

	fn wave_collapse(&self, pos: GlobalPos) -> WorldChunk
	{
		let generate = pos.0.z == 0;

		let chunk = if generate
		{
			WorldChunk::new(fastrand::usize(1..self.chunk_rules.len()))
		} else
		{
			WorldChunk::none()
		};

		self.chunk_saver.save(pos, &chunk);

		chunk
	}

	fn load_missing<F>(
		&self,
		world_chunks: &mut ChunksContainer<Option<WorldChunk>>,
		mut to_global: F
	)
	where
		F: FnMut(LocalPos) -> GlobalPos
	{
		world_chunks.iter_mut().filter(|(_, chunk)| chunk.is_none())
			.for_each(|(pos, chunk)|
			{
				let loaded_chunk = self.chunk_saver.load(to_global(pos));

				loaded_chunk.map(|loaded_chunk|
				{
					*chunk = Some(loaded_chunk);
				});
			});
	}

	pub fn generate_chunk(
		&self,
		group: AlwaysGroup<WorldChunk>
	) -> ChunksContainer<Tile>
	{
		self.chunk_generator.generate_chunk(group.map(|world_chunk|
		{
			&self.chunk_rules[world_chunk.id()].name[..]
		}))
	}
}
