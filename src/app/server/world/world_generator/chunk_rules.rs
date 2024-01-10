use std::{
    iter,
    mem,
    slice,
    io::Write,
    fs::File,
    fmt::{self, Debug},
    path::{Path, PathBuf},
    collections::HashMap,
    ops::Index
};

use serde::Deserialize;

use super::{PossibleStates, ParseError};

use crate::common::world::{
    CHUNK_SIZE,
    GlobalPos,
    Pos3,
    DirectionsGroup,
    chunk::PosDirection
};


pub const WORLD_CHUNK_SIZE: Pos3<usize> = Pos3{x: 16, y: 16, z: 1};
pub const CHUNK_RATIO: Pos3<usize> = Pos3{
    x: CHUNK_SIZE / WORLD_CHUNK_SIZE.x,
    y: CHUNK_SIZE / WORLD_CHUNK_SIZE.y,
    z: CHUNK_SIZE / WORLD_CHUNK_SIZE.z
};

#[repr(C, u8)]
#[derive(Debug, Clone, Copy)]
pub enum MaybeWorldChunk
{
    None,
    Some(WorldChunk)
}

impl From<MaybeWorldChunk> for Option<WorldChunk>
{
    fn from(value: MaybeWorldChunk) -> Self
    {
        match value
        {
            MaybeWorldChunk::None => None,
            MaybeWorldChunk::Some(value) => Some(value)
        }
    }
}

impl MaybeWorldChunk
{
    pub const fn size_of() -> usize
    {
        mem::size_of::<Self>()
    }

    pub const fn index_of(index: usize) -> usize
    {
        index * Self::size_of()
    }

    pub fn write_into(self, mut writer: impl Write)
    {
        let size = mem::size_of::<Self>();
        let bytes: &[u8] = unsafe{
            slice::from_raw_parts(&self as *const Self as *const u8, size)
        };

        writer.write_all(bytes).unwrap();
    }

    pub fn from_bytes(bytes: &[u8]) -> Self
    {
        assert_eq!(bytes.len(), mem::size_of::<Self>());

        unsafe{
            (bytes.as_ptr() as *const Self).read()
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldChunkId(usize);

impl fmt::Display for WorldChunkId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", self.0)
    }
}

impl WorldChunkId
{
    #[cfg(test)]
    pub fn from_raw(id: usize) -> Self
    {
        Self(id)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldChunk
{
	id: WorldChunkId
}

impl WorldChunk
{
    pub fn new(id: WorldChunkId) -> Self
    {
        Self{id}
    }

	#[allow(dead_code)]
	pub fn none() -> Self
	{
		Self{id: WorldChunkId(0)}
	}

    pub fn is_none(&self) -> bool
    {
        self.id.0 == 0
    }

	pub fn id(&self) -> WorldChunkId
	{
		self.id
	}

    pub fn belongs_to(pos: GlobalPos) -> GlobalPos
    {
        GlobalPos::from(pos.0.zip(CHUNK_RATIO).map(|(value, ratio)|
        {
            let ratio = ratio as i32;

            if value < 0
            {
                value / ratio - 1
            } else
            {
                value / ratio
            }
        }))
    }

    pub fn global_to_index(pos: GlobalPos) -> usize
    {
        let local_pos = pos.0.zip(CHUNK_RATIO).map(|(x, ratio)|
        {
            let m = x % ratio as i32;

            if m < 0
            {
                (ratio as i32 + m) as usize
            } else
            {
                m as usize
            }
        });

        local_pos.z * CHUNK_RATIO.y * CHUNK_RATIO.x
            + local_pos.y * CHUNK_RATIO.x
            + local_pos.x
    }
}

#[derive(Debug, Deserialize)]
pub struct ChunkRuleRaw
{
	pub name: String,
    pub weight: f64,
	pub neighbors: DirectionsGroup<Vec<String>>
}

#[derive(Debug, Deserialize)]
pub struct ChunkRulesRaw
{
    rules: Vec<ChunkRuleRaw>,
    fallback: String
}

#[derive(Debug, Clone)]
struct ChunkRule
{
    name: String,
    weight: f64,
	neighbors: DirectionsGroup<Vec<WorldChunkId>>
}

impl ChunkRule
{
    fn from_raw(name_mappings: &NameMappings, rule: ChunkRuleRaw, total_weight: f64) -> Self
    {
        let ChunkRuleRaw{
            name,
            weight,
            neighbors
        } = rule;

        Self{
            name,
            weight: weight / total_weight,
            neighbors: neighbors.map(|_, direction|
            {
                direction.into_iter().map(|name|
                {
                    name_mappings[&name]
                }).collect::<Vec<_>>()
            })
        }
    }
}

pub struct BorrowedChunkRule<'a>
{
    rules: &'a ChunkRules,
    rule: &'a ChunkRule
}

impl<'a> BorrowedChunkRule<'a>
{
    pub fn name(&self) -> &str
    {
        &self.rule.name
    }

    pub fn weight(&self) -> f64
    {
        self.rule.weight
    }

    pub fn neighbors(&self, direction: PosDirection) -> &[WorldChunkId]
    {
        &self.rule.neighbors[direction]
    }
}

#[derive(Debug)]
pub struct UndergroundRules(ChunkRules);

impl UndergroundRules
{
    fn load(
        name_mappings: &NameMappings,
        file: File
    ) -> Result<Self, serde_json::Error>
    {
		let rules = serde_json::from_reader::<_, ChunkRulesRaw>(file)?;

        Ok(Self::from_raw(name_mappings, rules))
    }

    fn from_raw(
        name_mappings: &NameMappings,
        rules: ChunkRulesRaw
    ) -> Self
    {
        Self(ChunkRules::from_raw(name_mappings, rules))
    }

    pub fn fallback(&self) -> WorldChunkId
    {
        self.0.fallback
    }
}

#[derive(Debug)]
pub struct CityRules
{
}

impl CityRules
{
    fn load(
        _name_mappings: &NameMappings,
        _file: File
    ) -> Result<Self, serde_json::Error>
    {
        Ok(Self{})
    }
}

struct NameMappings(HashMap<String, WorldChunkId>);

impl FromIterator<(String, WorldChunkId)> for NameMappings
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item=(String, WorldChunkId)>
    {
        Self(HashMap::from_iter(iter))
    }
}

impl Index<&str> for NameMappings
{
    type Output = WorldChunkId;

    fn index(&self, index: &str) -> &Self::Output
    {
        self.0.get(index).unwrap_or_else(||
        {
            panic!("worldchunk '{index}' not found")
        })
    }
}

#[derive(Debug)]
pub struct ChunkRulesGroup
{
    world_chunks: Box<[String]>,
    pub ground: ChunkRules,
    pub underground: UndergroundRules,
    pub city: CityRules
}

impl ChunkRulesGroup
{
    pub fn load(path: PathBuf) -> Result<Self, ParseError>
    {
        // holy iterator
        let world_chunks = iter::once(Ok("none".to_owned())).chain(path.join("chunks")
            .read_dir()?
            .filter(|entry|
            {
                entry.as_ref().ok().and_then(|entry|
                {
                    entry.file_type().ok()
                }).map(|filetype| filetype.is_file())
                .unwrap_or(true)
            })
            .map(|entry|
            {
                entry.map(|entry|
                {
                    let filename = entry.file_name();
                    let path: &Path = filename.as_ref();

                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
            }))
            .collect::<Result<Box<[String]>, _>>()?;

        let name_mappings = world_chunks.iter().enumerate().map(|(index, name)|
        {
            (name.clone(), WorldChunkId(index))
        }).collect::<NameMappings>();


        Ok(Self{
            world_chunks,
            ground: Self::load_rules(path.join("ground.json"), |file|
            {
                ChunkRules::load(&name_mappings, file)
            })?,
            underground: Self::load_rules(path.join("underground.json"), |file|
            {
                UndergroundRules::load(&name_mappings, file)
            })?,
            city: Self::load_rules(path.join("city.json"), |file|
            {
                CityRules::load(&name_mappings, file)
            })?
        })
    }

    fn load_rules<F, T>(path: PathBuf, f: F) -> Result<T, ParseError>
    where
        F: FnOnce(File) -> Result<T, serde_json::Error>
    {
        let file = File::open(&path).map_err(|err|
        {
            ParseError::new_named(path.to_owned(), err)
        })?;

        f(file).map_err(|err|
        {
            ParseError::new_named(path.to_owned(), err)
        })
    }

    pub fn name(&self, id: WorldChunkId) -> &str
    {
        &self.world_chunks[id.0]
    }

    pub fn iter_names(&self) -> impl Iterator<Item=&String>
    {
        self.world_chunks.iter()
    }
}

#[derive(Debug)]
pub struct ChunkRules
{
    rules: HashMap<WorldChunkId, ChunkRule>,
    fallback: WorldChunkId,
    total_weight: f64,
    entropy: f64
}

impl ChunkRules
{
    fn load(
        name_mappings: &NameMappings,
        file: File
    ) -> Result<Self, serde_json::Error>
    {
		let rules = serde_json::from_reader::<_, ChunkRulesRaw>(file)?;

        Ok(Self::from_raw(name_mappings, rules))
    }

    fn from_raw(name_mappings: &NameMappings, rules: ChunkRulesRaw) -> Self
    {
        let weights = rules.rules.iter().map(|rule| rule.weight);

        let total_weight: f64 = weights.clone().sum();
        let entropy = PossibleStates::calculate_entropy(weights);

        let ChunkRulesRaw{
            rules,
            fallback
        } = rules;

        Self{
            total_weight: 1.0,
            entropy,
            fallback: name_mappings[&fallback],
            rules: rules.into_iter().map(|rule|
            {
                let rule = ChunkRule::from_raw(name_mappings, rule, total_weight);
                let id = name_mappings[&rule.name];

                (id, rule)
            }).collect::<HashMap<WorldChunkId, ChunkRule>>()
        }
    }

    pub fn ids(&self) -> impl Iterator<Item=&WorldChunkId>
    {
        self.rules.keys()
    }

    pub fn name(&self, id: WorldChunkId) -> &str
    {
        &self.rules.get(&id).unwrap_or_else(|| panic!("{id} out of range")).name
    }

    pub fn total_weight(&self) -> f64
    {
        self.total_weight
    }

    pub fn entropy(&self) -> f64
    {
        self.entropy
    }

    pub fn fallback(&self) -> WorldChunkId
    {
        self.fallback
    }

    pub fn len(&self) -> usize
    {
        self.rules.len()
    }

    pub fn get_maybe(&self, id: WorldChunkId) -> Option<BorrowedChunkRule<'_>>
    {
        self.rules.get(&id).map(|rule|
        {
            BorrowedChunkRule{
                rules: self,
                rule
            }
        })
    }

    pub fn get(&self, id: WorldChunkId) -> BorrowedChunkRule<'_>
    {
        self.get_maybe(id).unwrap_or_else(|| panic!("{id} out of range"))
    }

    pub fn iter(&self) -> impl Iterator<Item=BorrowedChunkRule<'_>> + '_
    {
        self.rules.values().map(move |rule|
        {
            BorrowedChunkRule{
                rules: self,
                rule
            }
        })
    }
}
