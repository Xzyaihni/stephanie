use std::{
    iter,
    mem,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn take_tags(&mut self) -> Vec<WorldChunkTag>
    {
        self.0.as_mut().map(WorldChunk::take_tags).unwrap_or_default()
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
        // using the MAX const so it doesnt give wrong size if i wanna use varint
        Self::options_prelimit().serialized_size(
            &Self(Some(WorldChunk::new(
                WorldChunkId(usize::MAX),
                Vec::new()
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
#[serde(transparent)]
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
#[serde(transparent)]
pub struct TextId(usize);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TagContent
{
    Number(i64),
    Range(Range<i64>),
    Text(TextId),
    Boolean(bool)
}

impl From<&RuleTagContent> for TagContent
{
    fn from(value: &RuleTagContent) -> Self
    {
        match value
        {
            RuleTagContent::Number(x) => Self::Number(*x),
            RuleTagContent::Range(x) => Self::Range(x.clone()),
            RuleTagContent::Random(x) => Self::Number(fastrand::i64(x.clone())),
            RuleTagContent::Text(x) => Self::Text(*x),
            RuleTagContent::Boolean(x) => Self::Boolean(*x)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldChunkTag
{
    name: TextId,
    content: TagContent
}

impl From<&ChunkRuleTag> for WorldChunkTag
{
    fn from(tag: &ChunkRuleTag) -> Self
    {
        Self{
            name: tag.name,
            content: TagContent::from(&tag.content)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorldChunk
{
	id: WorldChunkId,
    #[serde(skip)]
    tags: Vec<WorldChunkTag>
}

impl WorldChunk
{
    pub fn new(id: WorldChunkId, tags: Vec<WorldChunkTag>) -> Self
    {
        Self{id, tags}
    }

	#[allow(dead_code)]
	pub fn none() -> Self
	{
		Self{id: WorldChunkId(0), tags: Vec::new()}
	}

    pub fn is_none(&self) -> bool
    {
        self.id.0 == 0
    }

	pub fn id(&self) -> WorldChunkId
	{
		self.id
	}

    pub fn take_tags(&mut self) -> Vec<WorldChunkTag>
    {
        mem::take(&mut self.tags)
    }

    pub fn with_tags(self, tags: Vec<WorldChunkTag>) -> Self
    {
        Self{
            tags,
            ..self
        }
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
pub enum RuleRawTagContent
{
    Number(i64),
    Range(Range<i64>),
    Random(Range<i64>),
    Text(String),
    Boolean(bool)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChunkRuleRawTag
{
    name: String,
    content: RuleRawTagContent
}

#[derive(Debug, Deserialize)]
pub struct ChunkRuleRaw
{
	pub name: String,
    #[serde(default)]
    pub tags: Vec<ChunkRuleRawTag>,
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
pub enum RuleTagContent
{
    Number(i64),
    Range(Range<i64>),
    Random(Range<i64>),
    Text(TextId),
    Boolean(bool)
}

impl RuleTagContent
{
    fn from_raw(text_mapping: &mut TextMapping, raw_tag: RuleRawTagContent) -> Self
    {
        // not sure if i should just make this a generic >_<
        match raw_tag
        {
            RuleRawTagContent::Number(x) => Self::Number(x),
            RuleRawTagContent::Range(x) => Self::Range(x),
            RuleRawTagContent::Random(x) => Self::Random(x),
            RuleRawTagContent::Text(x) => Self::Text(text_mapping.to_id(x)),
            RuleRawTagContent::Boolean(x) => Self::Boolean(x)
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkRuleTag
{
    name: TextId,
    content: RuleTagContent
}

impl ChunkRuleTag
{
    fn from_raw(text_mapping: &mut TextMapping, raw_tag: ChunkRuleRawTag) -> Self
    {
        Self{
            name: text_mapping.to_id(raw_tag.name),
            content: RuleTagContent::from_raw(text_mapping, raw_tag.content)
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkRule
{
    name: String,
    tags: Vec<ChunkRuleTag>,
    weight: f64,
	neighbors: DirectionsGroup<Vec<WorldChunkId>>
}

impl ChunkRule
{
    fn from_raw(name_mappings: &mut NameMappings, rule: ChunkRuleRaw, total_weight: f64) -> Self
    {
        Self{
            name: rule.name,
            tags: rule.tags.into_iter().map(|tag|
            {
                ChunkRuleTag::from_raw(&mut name_mappings.text, tag)
            }).collect(),
            weight: rule.weight / total_weight,
            neighbors: rule.neighbors.map(|_, direction|
            {
                direction.into_iter().map(|name|
                {
                    name_mappings.world_chunk[&name]
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
        name_mappings: &mut NameMappings,
        file: File
    ) -> Result<Self, serde_json::Error>
    {
		let rules = serde_json::from_reader::<_, ChunkRulesRaw>(file)?;

        Ok(Self::from_raw(name_mappings, rules))
    }

    fn from_raw(
        name_mappings: &mut NameMappings,
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

#[derive(Debug, Deserialize)]
enum ConditionalVariable
{
    Height
}

// this is literally just cmp::Ordering, but it doesnt implement deserialize >-<
#[derive(Debug, Deserialize)]
enum ConditionalCondition
{
    Less,
    Equal,
    Greater
}

#[derive(Debug, Deserialize)]
struct ConditionalRuleRaw
{
    name: String,
    variable: ConditionalVariable,
    tag: String,
    condition: ConditionalCondition
}

#[derive(Debug, Deserialize)]
struct CityRulesRaw
{
    rules: Vec<ConditionalRuleRaw>
}

#[derive(Debug)]
struct ConditionalRule
{
    name: WorldChunkId,
    variable: ConditionalVariable,
    tag: TextId,
    condition: ConditionalCondition
}

impl ConditionalRule
{
    fn from_raw(name_mappings: &NameMappings, rule: ConditionalRuleRaw) -> Self
    {
        Self{
            name: name_mappings.world_chunk[&rule.name],
            variable: rule.variable,
            condition: rule.condition,
            tag: name_mappings.text[&rule.tag]
        }
    }
}

#[derive(Debug)]
pub struct CityRules
{
    rules: Vec<ConditionalRule>
}

impl CityRules
{
    fn load(
        name_mappings: &NameMappings,
        file: File
    ) -> Result<Self, serde_json::Error>
    {
		let rules = serde_json::from_reader::<_, CityRulesRaw>(file)?;

        Ok(Self::from_raw(name_mappings, rules))
    }

    fn from_raw(name_mappings: &NameMappings, rules: CityRulesRaw) -> Self
    {
        // rules rules rules!!!!!!
        Self{
            rules: rules.rules.into_iter().map(|rule|
            {
                ConditionalRule::from_raw(name_mappings, rule)
            }).collect()
        }
    }
}

struct NameIndexer<T>(HashMap<String, T>);

impl<T> NameIndexer<T>
{
    pub fn new() -> Self
    {
        Self(HashMap::new())
    }

    pub fn insert(&mut self, key: String, value: T)
    {
        self.0.insert(key, value);
    }
}

impl<T> FromIterator<(String, T)> for NameIndexer<T>
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item=(String, T)>
    {
        Self(HashMap::from_iter(iter))
    }
}

impl<T> Index<&str> for NameIndexer<T>
{
    type Output = T;

    fn index(&self, index: &str) -> &Self::Output
    {
        self.0.get(index).unwrap_or_else(||
        {
            panic!("'{index}' not found")
        })
    }
}

struct TextMapping
{
    text: Vec<String>,
    indexer: NameIndexer<TextId>
}

impl TextMapping
{
    pub fn new() -> Self
    {
        Self{text: Vec::new(), indexer: NameIndexer::new()}
    }

    pub fn to_id(&mut self, value: String) -> TextId
    {
        let id = TextId(self.text.len());

        self.text.push(value.clone());
        self.indexer.insert(value, id);

        id
    }
}

impl Index<&str> for TextMapping
{
    type Output = TextId;

    fn index(&self, index: &str) -> &Self::Output
    {
        self.indexer.index(index)
    }
}

struct NameMappings
{
    pub world_chunk: NameIndexer<WorldChunkId>,
    pub text: TextMapping
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

        let mut name_mappings = {
            let world_chunk = world_chunks.iter().enumerate().map(|(index, name)|
            {
                (name.clone(), WorldChunkId(index))
            }).collect::<NameIndexer<WorldChunkId>>();

            NameMappings{
                world_chunk,
                text: TextMapping::new()
            }
        };


        Ok(Self{
            world_chunks,
            surface: Self::load_rules(path.join("surface.json"), |file|
            {
                ChunkRules::load(&mut name_mappings, file)
            })?,
            underground: Self::load_rules(path.join("underground.json"), |file|
            {
                UndergroundRules::load(&mut name_mappings, file)
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
        name_mappings: &mut NameMappings,
        file: File
    ) -> Result<Self, serde_json::Error>
    {
		let rules = serde_json::from_reader::<_, ChunkRulesRaw>(file)?;

        Ok(Self::from_raw(name_mappings, rules))
    }

    fn from_raw(name_mappings: &mut NameMappings, rules: ChunkRulesRaw) -> Self
    {
        let weights = rules.rules.iter().map(|rule| rule.weight);

        let total_weight: f64 = weights.clone().sum();
        let entropy = PossibleStates::calculate_entropy(weights);

        Self{
            total_weight: 1.0,
            entropy,
            fallback: name_mappings.world_chunk[&rules.fallback],
            rules: rules.rules.into_iter().map(|rule|
            {
                let rule = ChunkRule::from_raw(name_mappings, rule, total_weight);
                let id = name_mappings.world_chunk[&rule.name];

                (id, rule)
            }).collect::<HashMap<WorldChunkId, ChunkRule>>()
        }
    }

    pub fn generate(&self, id: WorldChunkId) -> WorldChunk
    {
        let rule = self.get(id);

        WorldChunk::new(id, rule.tags.iter().map(WorldChunkTag::from).collect())
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
