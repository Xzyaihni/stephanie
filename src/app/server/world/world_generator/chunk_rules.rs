use std::{
    iter,
    io::Write,
    fs::File,
    fmt::{self, Debug},
    path::{Path, PathBuf},
    collections::HashMap,
    ops::{Range, Index}
};

use serde::{Serialize, Deserialize};

use bincode::Options;

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MaybeWorldChunk(pub Option<WorldChunk>);

impl From<WorldChunk> for MaybeWorldChunk
{
    fn from(value: WorldChunk) -> Self
    {
        Self(Some(value))
    }
}

impl From<MaybeWorldChunk> for Option<WorldChunk>
{
    fn from(value: MaybeWorldChunk) -> Self
    {
        value.0
    }
}

impl Default for MaybeWorldChunk
{
    fn default() -> Self
    {
        Self::none()
    }
}

impl MaybeWorldChunk
{
    pub fn none() -> Self
    {
        Self(None)
    }

    fn options_prelimit() -> impl Options
    {
        bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .allow_trailing_bytes()
    }

    fn options() -> impl Options
    {
        Self::options_prelimit().with_limit(Self::size_of() as u64)
    }

    pub fn size_of() -> usize
    {
        // usize::MAX is the same size as 0 but maybe i wanna use varint encoding later
        Self::options_prelimit().serialized_size(
            &Self(Some(WorldChunk::new(
                WorldChunkId(usize::MAX),
                Some(WorldChunkTag::Building{height: u64::MAX})
            )))
        ).unwrap() as usize
    }

    pub fn index_of(index: usize) -> usize
    {
        index * Self::size_of()
    }

    pub fn write_into(self, mut writer: impl Write)
    {
        let mut bytes = Self::options().serialize(&self).unwrap();

        let size = Self::size_of();

        assert!(bytes.len() <= size);

        bytes.resize_with(size, Default::default);

        assert_eq!(bytes.len(), size);

        writer.write_all(&bytes).unwrap();
    }

    pub fn from_bytes(bytes: &[u8]) -> Self
    {
        assert_eq!(bytes.len(), Self::size_of());

        Self::options().deserialize(bytes).unwrap()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldChunkTag
{
    Building{height: u64}
}

impl From<ChunkRuleTag> for WorldChunkTag
{
    fn from(value: ChunkRuleTag) -> Self
    {
        match value
        {
            ChunkRuleTag::Building{height} => Self::Building{height: fastrand::u64(height)}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldChunk
{
	id: WorldChunkId,
    tags: Option<WorldChunkTag>
}

impl WorldChunk
{
    pub fn new(id: WorldChunkId, tags: Option<WorldChunkTag>) -> Self
    {
        Self{id, tags}
    }

	#[allow(dead_code)]
	pub fn none() -> Self
	{
		Self{id: WorldChunkId(0), tags: None}
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum ChunkRuleTag
{
    Building{height: Range<u64>}
}

#[derive(Debug, Deserialize)]
pub struct ChunkRuleRaw
{
	pub name: String,
    #[serde(default)]
    pub tags: Option<ChunkRuleTag>,
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
pub struct ChunkRule
{
    name: String,
    tags: Option<ChunkRuleTag>,
    weight: f64,
	neighbors: DirectionsGroup<Vec<WorldChunkId>>
}

impl ChunkRule
{
    fn from_raw(name_mappings: &NameMappings, rule: ChunkRuleRaw, total_weight: f64) -> Self
    {
        let ChunkRuleRaw{
            name,
            tags,
            weight,
            neighbors
        } = rule;

        Self{
            name,
            tags,
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

    pub fn name(&self) -> &str
    {
        &self.name
    }

    pub fn weight(&self) -> f64
    {
        self.weight
    }

    pub fn neighbors(&self, direction: PosDirection) -> &[WorldChunkId]
    {
        &self.neighbors[direction]
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
    pub surface: ChunkRules,
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
            surface: Self::load_rules(path.join("surface.json"), |file|
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

    pub fn generate(&self, id: WorldChunkId) -> WorldChunk
    {
        let rule = self.get(id);

        WorldChunk::new(id, rule.tags.clone().map(WorldChunkTag::from))
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

    pub fn get_maybe(&self, id: WorldChunkId) -> Option<&ChunkRule>
    {
        self.rules.get(&id)
    }

    pub fn get(&self, id: WorldChunkId) -> &ChunkRule
    {
        self.get_maybe(id).unwrap_or_else(|| panic!("{id} out of range"))
    }

    pub fn iter(&self) -> impl Iterator<Item=&ChunkRule> + '_
    {
        self.rules.values()
    }
}
