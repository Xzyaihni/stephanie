use std::{
    mem,
    rc::Rc,
    io::Write,
    fs::File,
    fmt::{self, Debug},
    path::PathBuf,
    collections::HashMap,
    ops::{Range, Index}
};

use serde::{Serialize, Deserialize};

use strum::IntoEnumIterator;

use super::{PossibleState, ParseError};

use crate::{
    debug_config::*,
    common::{
        some_or_return,
        BiMap,
        ObjectsStore,
        generic_info::*,
        lisp::{self, LispConfig, Program, Primitives, LispMemory, LispValue, Register},
        world::{
            CHUNK_SIZE,
            LocalPos,
            GlobalPos,
            TileRotation,
            Pos3,
            DirectionsGroup,
            chunk::PosDirection
        }
    }
};


pub const WORLD_CHUNK_SIZE: Pos3<usize> = Pos3{x: 8, y: 8, z: 1};
pub const CHUNK_RATIO: Pos3<usize> = Pos3{
    x: CHUNK_SIZE / WORLD_CHUNK_SIZE.x,
    y: CHUNK_SIZE / WORLD_CHUNK_SIZE.y,
    z: CHUNK_SIZE / WORLD_CHUNK_SIZE.z
};

const ROTATEABLE_DEFAULT: bool = true;

fn union<T: PartialEq>(values: &mut Vec<T>, value: T) -> bool
{
    let has_value = values.contains(&value);
    if !has_value
    {
        values.push(value);
    }

    !has_value
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorldChunkId(usize);

impl fmt::Display for WorldChunkId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", self.0)
    }
}

impl Debug for WorldChunkId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "WorldChunkId({})", self.0)
    }
}

impl WorldChunkId
{
    pub fn none() -> Self
    {
        Self(0)
    }

    #[cfg(test)]
    pub fn from_raw(id: usize) -> Self
    {
        Self(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TextId(usize);

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldChunkTag
{
    name: TextId,
    content: i32
}

impl Debug for WorldChunkTag
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "({} {})", self.name.0, self.content)
    }
}

impl WorldChunkTag
{
    pub fn from_raw(name: TextId, content: i32) -> Self
    {
        Self{name, content}
    }

    fn generate_content(value: &Program) -> i32
    {
        value.eval(|_| {}).unwrap_or_else(|err|
        {
            panic!("lisp error {err}")
        }).as_integer().unwrap_or_else(|err|
        {
            panic!("{err}")
        })
    }

    fn generate(tag: &ChunkRuleTag) -> Self
    {
        Self{
            name: tag.name,
            content: Self::generate_content(&tag.content)
        }
    }

    pub fn as_lisp_value(
        &self,
        mappings: &NameMappings,
        memory: &mut LispMemory
    ) -> Result<LispValue, lisp::Error>
    {
        let name = mappings.text.get_name(self.name);
        let name = memory.new_symbol(name);

        let restore = memory.with_saved_registers([Register::Value, Register::Temporary]);

        memory.set_register(Register::Value, self.content);
        memory.set_register(Register::Temporary, name);

        memory.cons(Register::Value, Register::Temporary, Register::Value)?;

        let value = memory.get_register(Register::Value);

        restore(memory)?;

        Ok(value)
    }

    pub fn define(
        &self,
        mappings: &NameMappings,
        memory: &mut LispMemory
    ) -> Result<(), lisp::Error>
    {
        let name = mappings.text.get_name(self.name);

        memory.define(name, self.content.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldChunk
{
    id: WorldChunkId,
    tags: Vec<WorldChunkTag>
}

impl Default for WorldChunk
{
    fn default() -> Self
    {
        Self::none()
    }
}

impl WorldChunk
{
    pub const fn new(id: WorldChunkId, tags: Vec<WorldChunkTag>) -> Self
    {
        Self{id, tags}
    }

    #[allow(dead_code)]
    pub const fn none() -> Self
    {
        Self{id: WorldChunkId(0), tags: Vec::new()}
    }

    pub const fn is_none(&self) -> bool
    {
        self.id.0 == 0
    }

    pub const fn id(&self) -> WorldChunkId
    {
        self.id
    }

    pub fn tags(&self) -> &[WorldChunkTag]
    {
        &self.tags
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

    pub const fn size_of() -> usize
    {
        // id (u32)
        (u32::BITS / 8) as usize
    }

    pub fn index_of(index: usize) -> usize
    {
        index * Self::size_of()
    }

    pub fn write_into(self, mut writer: impl Write)
    {
        let bytes: [_; Self::size_of()] = (self.id.0 as u32).to_le_bytes();

        writer.write_all(&bytes).unwrap();
    }

    pub fn from_bytes(bytes: [u8; Self::size_of()]) -> Self
    {
        WorldChunk::new(WorldChunkId(u32::from_le_bytes(bytes) as usize), Vec::new())
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

    pub fn format_compact(&self) -> String
    {
        let tags = self.tags.iter().map(|x| format!("{x:?}")).reduce(|acc, x|
        {
            acc + " " + &x
        });

        if let Some(tags) = tags
        {
            format!("{} [{tags}]", self.id)
        } else
        {
            self.id.to_string()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChunkRuleRawTag
{
    name: String,
    content: String
}

// im using the same prefix for the json to be more readable
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Deserialize)]
enum ChunkNeighborsGeneric<T>
{
    SymmetryNone(DirectionsGroup<T>),
    SymmetryHorizontal{up: T, down: T, horizontal: T},
    SymmetryVertical{right: T, left: T, vertical: T},
    SymmetryBoth{horizontal: T, vertical: T},
    SymmetryAll(T)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, rename = "Rotated")]
enum MaybeRotatedNeighbor
{
    Normal(String),
    WithRotation{rotation: TileRotation, value: String}
}

impl From<MaybeRotatedNeighbor> for (TileRotation, String)
{
    fn from(value: MaybeRotatedNeighbor) -> Self
    {
        match value
        {
            MaybeRotatedNeighbor::Normal(x) => (TileRotation::Up, x),
            MaybeRotatedNeighbor::WithRotation{rotation, value} => (rotation, value)
        }
    }
}

type ChunkNeighborType = Vec<MaybeRotatedNeighbor>;
type ChunkNeighbors = ChunkNeighborsGeneric<ChunkNeighborType>;

impl ChunkNeighbors
{
    fn symmetry(&self) -> Symmetry
    {
        match self
        {
            Self::SymmetryNone(_) => Symmetry::None,
            Self::SymmetryHorizontal{..} => Symmetry::Horizontal,
            Self::SymmetryVertical{..} => Symmetry::Vertical,
            Self::SymmetryBoth{..} => Symmetry::Both,
            Self::SymmetryAll(_) => Symmetry::All
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkRuleTrackerRaw
{
    pub direction: Option<TileRotation>,
    pub neighbor: TileRotation
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ChunkWeightRaw
{
    Value(f64),
    Lisp(String)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkRuleRaw
{
    pub name: String,
    pub inherit: Option<String>,
    pub rotation: Option<TileRotation>,
    pub tags: Option<Vec<ChunkRuleRawTag>>,
    pub weight: Option<ChunkWeightRaw>,
    pub rotateable: Option<bool>,
    pub neighbors: Option<ChunkNeighbors>,
    pub track: Option<ChunkRuleTrackerRaw>
}

impl ChunkRuleRaw
{
    fn combine(&self, other: &Self) -> Self
    {
        let mut this = self.clone();

        this.name = other.name.clone();

        inherit_with_fields!(
            this,
            other,
            tags,
            weight,
            rotateable,
            neighbors
        );

        this
    }
}

#[derive(Debug, Deserialize)]
pub struct ChunkRulesRaw
{
    rules: Vec<ChunkRuleRaw>,
    fallback: String
}

#[derive(Debug, Clone)]
pub struct ChunkRuleTag
{
    name: TextId,
    content: Program
}

impl ChunkRuleTag
{
    fn from_raw(
        text_mapping: &mut TextMapping,
        primitives: Rc<Primitives>,
        raw_tag: ChunkRuleRawTag
    ) -> Self
    {
        let content = Program::parse(
            LispConfig{
                memory: LispMemory::new(primitives, 64, 64),
                ..Default::default()
            },
            &[&raw_tag.content]
        ).unwrap_or_else(|err|
        {
            panic!("error evaluating program: {err}")
        });

        Self{
            name: text_mapping.to_id(raw_tag.name),
            content
        }
    }
}

#[derive(Debug, Clone)]
enum ChunkWeight
{
    Value(f64),
    Lisp(Program)
}

#[derive(Debug, Clone)]
pub struct ChunkRule
{
    name: String,
    rotation: TileRotation,
    tags: Vec<ChunkRuleTag>,
    weight: ChunkWeight,
    rotateable: bool,
    symmetry: Symmetry,
    neighbors: DirectionsGroup<Vec<WorldChunkId>>,
    track: Option<TileRotation>
}

impl ChunkRule
{
    fn from_raw(
        name_mappings: &mut NameMappings,
        rule: ChunkRuleRaw
    ) -> Option<Self>
    {
        let neighbors = if let Some(x) = rule.neighbors
        {
            x
        } else
        {
            eprintln!("rule `{}` must have a neighbors field", &rule.name);

            return None;
        };

        let symmetry = neighbors.symmetry();

        let neighbors = {
            let n = |neighbors: ChunkNeighborType|
            {
                neighbors.into_iter().filter_map(|name|
                {
                    let neighbor = name_mappings.world_chunk.get(&name.clone().into()).cloned();

                    if neighbor.is_none()
                    {
                        eprintln!("{name:?} doesnt exist");
                    }

                    neighbor
                }).collect::<Vec<_>>()
            };

            match neighbors
            {
                ChunkNeighbors::SymmetryNone(x) => x.map(|_, x| n(x)),
                ChunkNeighbors::SymmetryHorizontal{up, down, horizontal} =>
                {
                    let horizontal = n(horizontal);
                    DirectionsGroup{
                        up: n(up),
                        down: n(down),
                        left: horizontal.clone(),
                        right: horizontal
                    }
                },
                ChunkNeighbors::SymmetryVertical{left, right, vertical} =>
                {
                    let vertical = n(vertical);
                    DirectionsGroup{
                        up: vertical.clone(),
                        down: vertical,
                        left: n(left),
                        right: n(right)
                    }
                },
                ChunkNeighbors::SymmetryBoth{horizontal, vertical} =>
                {
                    let horizontal = n(horizontal);
                    let vertical = n(vertical);
                    DirectionsGroup{
                        up: vertical.clone(),
                        down: vertical,
                        left: horizontal.clone(),
                        right: horizontal
                    }
                },
                ChunkNeighbors::SymmetryAll(x) => DirectionsGroup::repeat(n(x))
            }
        };

        let primitives = Rc::new(Primitives::default());

        let weight = rule.weight.and_then(|weight|
        {
            match weight
            {
                ChunkWeightRaw::Value(x) => Some(ChunkWeight::Value(x)),
                ChunkWeightRaw::Lisp(code) =>
                {
                    match Program::parse(
                        LispConfig{
                            memory: LispMemory::new(primitives.clone(), 64, 64),
                            ..Default::default()
                        },
                        &[&code]
                    )
                    {
                        Ok(program) => Some(ChunkWeight::Lisp(program)),
                        Err(err) =>
                        {
                            eprintln!("in `{}` error evaluating program: {err}", &rule.name);

                            None
                        }
                    }
                }
            }
        }).unwrap_or_else(|| ChunkWeight::Value(1.0));

        Some(Self{
            name: rule.name,
            rotation: TileRotation::Up,
            tags: rule.tags.map(|tags| tags.into_iter().map(|tag|
            {
                ChunkRuleTag::from_raw(
                    &mut name_mappings.text,
                    primitives.clone(),
                    tag
                )
            }).collect()).unwrap_or_default(),
            weight,
            rotateable: rule.rotateable.unwrap_or(ROTATEABLE_DEFAULT),
            symmetry,
            neighbors,
            track: None
        })
    }

    fn rotated(&self, name_mappings: &NameMappings, rotation: TileRotation) -> Self
    {
        let rotate_symmetry = rotation.is_horizontal();
        let symmetry = match self.symmetry
        {
            x @ Symmetry::None
            | x @ Symmetry::Both
            | x @ Symmetry::All => x,
            Symmetry::Horizontal if rotate_symmetry => Symmetry::Vertical,
            Symmetry::Vertical if rotate_symmetry => Symmetry::Horizontal,
            x => x
        };

        let neighbors = {
            let x = self.neighbors.clone();

            match rotation
            {
                TileRotation::Up => DirectionsGroup{left: x.left, right: x.right, up: x.up, down: x.down},
                TileRotation::Right => DirectionsGroup{left: x.down, right: x.up, up: x.left, down: x.right},
                TileRotation::Left => DirectionsGroup{left: x.up, right: x.down, up: x.right, down: x.left},
                TileRotation::Down => DirectionsGroup{left: x.right, right: x.left, up: x.down, down: x.up}
            }
        };

        let neighbors = neighbors.map(|_, neighbors|
        {
            let rotate = |id: WorldChunkId|
            {
                let (this_rotation, name) = name_mappings.world_chunk.get_back(&id).unwrap();

                *name_mappings.world_chunk.get(&(this_rotation.combine(rotation), name.clone())).unwrap()
            };

            neighbors.into_iter().map(rotate).collect::<Vec<_>>()
        });

        Self{
            rotation,
            neighbors,
            symmetry,
            ..self.clone()
        }
    }

    fn combine(&mut self, other: Self)
    {
        self.neighbors.as_mut().zip(other.neighbors).for_each(|_, (this, other)|
        {
            other.into_iter().for_each(|other|
            {
                union(this, other);
            });
        });
    }

    pub fn name(&self) -> &str
    {
        &self.name
    }

    pub fn rotation(&self) -> TileRotation
    {
        self.rotation
    }

    pub fn rotateable(&self) -> bool
    {
        self.rotateable
    }

    pub fn symmetry(&self) -> Symmetry
    {
        self.symmetry
    }

    pub fn neighbors(&self, direction: PosDirection) -> &[WorldChunkId]
    {
        &self.neighbors[direction]
    }
}

trait ParsableRules: Sized
{
    fn parse(name_mappings: &mut NameMappings, file: File) -> Result<Self, serde_json::Error>;
}

#[derive(Debug)]
pub struct UndergroundRules(ChunkRules);

impl UndergroundRules
{
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

impl ParsableRules for UndergroundRules
{
    fn parse(
        name_mappings: &mut NameMappings,
        file: File
    ) -> Result<Self, serde_json::Error>
    {
        let rules = serde_json::from_reader::<_, ChunkRulesRaw>(file)?;

        Ok(Self::from_raw(name_mappings, rules))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
enum ConditionalVariable
{
    Height
}

#[derive(Debug, Clone, Deserialize)]
enum RangeNumberRaw
{
    Number(i32),
    Constant(String)
}

#[derive(Debug, Deserialize)]
struct ConditionalRuleRaw
{
    name: String,
    variable: Option<ConditionalVariable>,
    range: Range<RangeNumberRaw>
}

#[derive(Debug, Deserialize)]
struct RuleConstant
{
    name: String,
    value: i32
}

#[derive(Debug, Deserialize)]
struct CityRulesRaw
{
    rules: Vec<ConditionalRuleRaw>,
    constants: Vec<RuleConstant>
}

#[derive(Debug, Clone, Copy)]
enum RangeNumber
{
    Number(i32)
}

impl RangeNumber
{
    fn from_raw(
        constants: &[RuleConstant],
        value: RangeNumberRaw
    ) -> Self
    {
        match value
        {
            RangeNumberRaw::Number(x) => Self::Number(x),
            RangeNumberRaw::Constant(name) =>
            {
                let n = constants.iter().find_map(|x|
                {
                    (x.name == name).then_some(x.value)
                }).unwrap_or_else(||
                {
                    eprintln!("couldnt find a constant named `{name}`, using 0 instead");

                    0
                });

                Self::Number(n)
            }
        }
    }

    fn as_number(&self) -> i32
    {
        match self
        {
            Self::Number(x) => *x
        }
    }
}

#[derive(Debug, Clone)]
struct ConditionalRule
{
    name: WorldChunkId,
    variable: ConditionalVariable,
    range: Range<RangeNumber>
}

impl ConditionalRule
{
    fn from_raw_empty(
        constants: &[RuleConstant],
        rule: ConditionalRuleRaw
    ) -> Self
    {
        Self{
            name: WorldChunkId::none(),
            variable: rule.variable.unwrap_or(ConditionalVariable::Height),
            range: Range{
                start: RangeNumber::from_raw(constants, rule.range.start),
                end: RangeNumber::from_raw(constants, rule.range.end)
            }
        }
    }

    pub fn with_id(self, id: WorldChunkId) -> Self
    {
        Self{
            name: id,
            ..self
        }
    }

    pub fn matches(
        &self,
        info: &ConditionalInfo,
        this: WorldChunkId
    ) -> bool
    {
        if self.name == this
        {
            let start = self.range.start.as_number();
            let end = self.range.end.as_number();

            (start..end).contains(&info.get_variable(self.variable))
        } else
        {
            false
        }
    }
}

#[derive(Debug)]
pub struct ConditionalInfo
{
    pub position: LocalPos,
    pub height: i32,
    pub difficulty: f32
}

impl ConditionalInfo
{
    fn get_variable(&self, variable: ConditionalVariable) -> i32
    {
        match variable
        {
            ConditionalVariable::Height => self.height
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
    fn from_raw(name_mappings: &mut NameMappings, rules: CityRulesRaw) -> Self
    {
        rules.rules.iter().for_each(|rule| name_mappings.insert_all(rule.name.clone()));

        let mut this_rules = Vec::new();

        rules.rules.into_iter().for_each(|rule|
        {
            let name = rule.name.clone();
            let rule = ConditionalRule::from_raw_empty(&rules.constants, rule);

            TileRotation::iter().for_each(|rotation|
            {
                let rule = rule.clone().with_id(name_mappings.world_chunk[&(rotation, name.clone())]);

                this_rules.push(rule);
            });
        });

        Self{
            rules: this_rules
        }
    }

    pub fn generate(&self, info: ConditionalInfo, this: WorldChunkId) -> WorldChunk
    {
        self.generate_underground(info, this).unwrap_or_default()
    }

    pub fn generate_underground(
        &self,
        info: ConditionalInfo,
        this: WorldChunkId
    ) -> Option<WorldChunk>
    {
        // imagine using find_map, couldnt be me
        self.rules.iter().find(|rule|
        {
            rule.matches(&info, this)
        }).map(|rule|
        {
            WorldChunk::new(rule.name, Vec::new())
        })
    }
}

impl ParsableRules for CityRules
{
    fn parse(
        name_mappings: &mut NameMappings,
        file: File
    ) -> Result<Self, serde_json::Error>
    {
        let rules = serde_json::from_reader::<_, CityRulesRaw>(file)?;

        Ok(Self::from_raw(name_mappings, rules))
    }
}

#[derive(Debug)]
pub struct NameIndexer<T>(HashMap<String, T>);

impl<T> NameIndexer<T>
{
    pub fn new() -> Self
    {
        Self(HashMap::new())
    }

    pub fn get(&self, key: &str) -> Option<&T>
    {
        self.0.get(key)
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

#[derive(Debug)]
pub struct TextMapping
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

    pub fn get(&self, index: &str) -> Option<TextId>
    {
        self.indexer.get(index).copied()
    }

    pub fn get_name(&self, id: TextId) -> &str
    {
        &self.text[id.0]
    }

    // this is the best name i could come up with, cmon :/
    #[allow(clippy::wrong_self_convention)]
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

fn format_id(name: &str, rotation: TileRotation) -> String
{
    if rotation == TileRotation::Up
    {
        name.to_owned()
    } else
    {
        format!("{}{name}", rotation.to_arrow_str())
    }
}

#[derive(Debug)]
pub struct NameMappings
{
    pub world_chunk: BiMap<(TileRotation, String), WorldChunkId>,
    pub text: TextMapping,
    current_index: usize
}

impl NameMappings
{
    fn format_id(&self, id: &WorldChunkId) -> String
    {
        let (direction, name) = self.world_chunk.get_back(id).unwrap();

        format_id(name, *direction)
    }

    fn insert_all(&mut self, name: String)
    {
        TileRotation::iter().for_each(|rotation|
        {
            self.insert(rotation, name.clone());
        });
    }

    fn insert(&mut self, rotation: TileRotation, name: String)
    {
        let key = (rotation, name);
        if self.world_chunk.contains_key(&key)
        {
            return;
        }

        let id = WorldChunkId(self.current_index);
        self.current_index += 1;

        self.world_chunk.insert(key, id);
    }

    pub fn id_by_rotation_name(&self, rotation: TileRotation, name: String) -> Option<WorldChunkId>
    {
        self.world_chunk.get(&(rotation, name)).copied()
    }
}

#[derive(Debug)]
pub struct ChunkRulesGroup
{
    name_mappings: NameMappings,
    pub surface: ChunkRules,
    pub underground: UndergroundRules,
    pub city: CityRules
}

impl ChunkRulesGroup
{
    pub fn load(path: PathBuf) -> Result<Self, ParseError>
    {
        let mut name_mappings = NameMappings{
            world_chunk: BiMap::new(),
            text: TextMapping::new(),
            current_index: 0
        };

        name_mappings.insert(TileRotation::Up, "none".to_owned());
        assert_eq!(name_mappings.world_chunk[&(TileRotation::Up, "none".to_owned())], WorldChunkId(0));

        Ok(Self{
            surface: Self::load_rules(&mut name_mappings, path.join("surface.json"))?,
            underground: Self::load_rules(&mut name_mappings, path.join("underground.json"))?,
            city: Self::load_rules(&mut name_mappings, path.join("city.json"))?,
            name_mappings
        })
    }

    #[cfg(test)]
    pub fn insert_chunk(&mut self, name: String)
    {
        self.name_mappings.insert(TileRotation::Up, name);
    }

    fn load_rules<T: ParsableRules>(
        name_mappings: &mut NameMappings,
        path: PathBuf
    ) -> Result<T, ParseError>
    {
        let file = File::open(&path).map_err(|err|
        {
            ParseError::new_named(path.to_owned(), err)
        })?;

        let rules = T::parse(name_mappings, file).map_err(|err|
        {
            ParseError::new_named(path.to_owned(), err)
        })?;

        Ok(rules)
    }

    pub fn name_mappings(&self) -> &NameMappings
    {
        &self.name_mappings
    }

    pub fn rotation(&self, id: WorldChunkId) -> TileRotation
    {
        self.name_mappings.world_chunk.get_back(&id).unwrap_or_else(||
        {
            panic!("id {id} doesnt exist")
        }).0
    }

    pub fn name(&self, id: WorldChunkId) -> &str
    {
        &self.name_mappings.world_chunk.get_back(&id).unwrap_or_else(||
        {
            panic!("id {id} doesnt exist")
        }).1
    }

    pub fn iter_names(&self) -> impl Iterator<Item=&(TileRotation, String)>
    {
        self.name_mappings.world_chunk.iter_front()
    }
}

#[derive(Debug)]
pub struct ChunkRules
{
    rules: ObjectsStore<ChunkRule>,
    fallback: WorldChunkId,
    all_ids: Vec<(WorldChunkId, Option<f64>)>
}

impl ChunkRules
{
    fn from_raw(name_mappings: &mut NameMappings, mut rules: ChunkRulesRaw) -> Self
    {
        inherit_infos(
            &mut rules.rules,
            |this_info| this_info.inherit.as_ref(),
            |this_info| &this_info.name,
            |a, b| a.combine(b)
        );

        rules.rules.iter().for_each(|rule|
        {
            if rule.rotateable.unwrap_or(ROTATEABLE_DEFAULT)
            {
                name_mappings.insert_all(rule.name.clone())
            } else
            {
                name_mappings.insert(TileRotation::Up, rule.name.clone())
            }
        });

        let fallback = name_mappings.world_chunk[&(TileRotation::Up, rules.fallback)];

        let mut this_rules: ObjectsStore<ChunkRule> = ObjectsStore::new();

        rules.rules.into_iter().for_each(|rule|
        {
            let override_rotation = rule.rotation;

            let track = rule.track.clone();

            let rule = some_or_return!(ChunkRule::from_raw(name_mappings, rule));

            let name_mappings = &*name_mappings;

            let is_rotateable = rule.rotateable;

            let has_override = override_rotation.is_some();

            let this_rules = &mut this_rules;
            let mut with_rotation = move |rotation|
            {
                let mut rule = if has_override { rule.clone() } else { rule.rotated(name_mappings, rotation) };
                if let Some(track) = track.as_ref()
                {
                    if track.direction.or(override_rotation).unwrap_or(TileRotation::Up) == rotation
                    {
                        rule.track = Some(track.neighbor);
                    }
                }

                let id = name_mappings.world_chunk[&(rotation, rule.name.clone())];

                if let Some(current) = this_rules.get_mut(id.0)
                {
                    if !has_override
                    {
                        return eprintln!("{} with no override cant come after rotation overriden rules", &rule.name);
                    }

                    if current.symmetry != rule.symmetry
                    {
                        return eprintln!("{} has a rotation override with an unrotated symmetry", &rule.name);
                    }

                    current.combine(rule);
                } else
                {
                    this_rules.insert(id.0, rule);
                }
            };

            if let Some(rotation) = override_rotation
            {
                with_rotation(rotation);
            } else
            {
                if is_rotateable
                {
                    TileRotation::iter().for_each(with_rotation);
                } else
                {
                    with_rotation(TileRotation::Up);
                }
            }
        });

        let mut all_ids: Vec<_> = this_rules.iter().map(|x| WorldChunkId(x.0)).map(|id|
        {
            let weight = match this_rules[id.0].weight
            {
                ChunkWeight::Value(x) => Some(x),
                _ => None
            };

            (id, weight)
        }).collect();

        all_ids.sort_by(|a, b| a.0.0.cmp(&b.0.0));

        let mut this = Self{
            rules: this_rules,
            all_ids,
            fallback
        };

        this.union_neighbors(name_mappings);

        if DebugConfig::is_enabled(DebugTool::PrintSurfaceRules)
        {
            this.print_neighbors(name_mappings);
        }

        this
    }

    fn self_symmetry_union(
        &mut self,
        name_mappings: &NameMappings,
        ids: &[WorldChunkId]
    ) -> bool
    {
        let mut changed = false;
        ids.iter().for_each(|this_id|
        {
            let this_symmetry = self.rules[this_id.0].symmetry;

            TileRotation::iter().for_each(|direction|
            {
                let pos_direction = direction.into();
                TileRotation::iter()
                    .filter(|x| *x != direction)
                    .filter(|x|
                    {
                        let is_horizontal = x.is_horizontal() && direction.is_horizontal();
                        let is_vertical = x.is_vertical() && direction.is_vertical();

                        match this_symmetry
                        {
                            Symmetry::None => false,
                            Symmetry::Horizontal => is_horizontal,
                            Symmetry::Vertical => is_vertical,
                            Symmetry::Both => is_horizontal || is_vertical,
                            Symmetry::All => true
                        }
                    })
                    .for_each(|other_direction|
                    {
                        (0..self.rules[this_id.0].neighbors[pos_direction].len()).for_each(|index|
                        {
                            let from_id = self.rules[this_id.0].neighbors[pos_direction][index];

                            match self.rules[from_id.0].symmetry
                            {
                                Symmetry::None => return,
                                Symmetry::Horizontal => if !direction.is_vertical() { return },
                                Symmetry::Vertical => if !direction.is_horizontal() { return },
                                Symmetry::Both | Symmetry::All => ()
                            }

                            let this = &mut self.rules[this_id.0];

                            let (this_rotation, this_name) = name_mappings.world_chunk.get_back(&from_id).unwrap();

                            let new_rotation = other_direction.subtract(direction).combine(*this_rotation);
                            if let Some(rotated_neighbor) = name_mappings.world_chunk.get(&(new_rotation, this_name.clone()))
                            {
                                let other_direction = other_direction.into();
                                if union(&mut this.neighbors[other_direction], *rotated_neighbor)
                                {
                                    changed = true;
                                    if let Some(track) = this.track.as_ref()
                                    {
                                        if PosDirection::from(*track) == other_direction
                                        {
                                            eprintln!(
                                                "{}: {other_direction} received {} from symmetrical rotation of {pos_direction}'s {}",
                                                name_mappings.format_id(this_id),
                                                name_mappings.format_id(rotated_neighbor),
                                                name_mappings.format_id(&from_id)
                                            );
                                        }
                                    }
                                }
                            }
                        });
                    });
            });
        });

        changed
    }

    fn rotation_symmetry_union(
        &mut self,
        name_mappings: &NameMappings,
        ids: &[WorldChunkId]
    ) -> bool
    {
        let mut changed = false;
        ids.iter().for_each(|this_id|
        {
            let (this_rotation, this_name) = name_mappings.world_chunk.get_back(this_id).unwrap();

            TileRotation::iter().filter(|x| *x != *this_rotation).for_each(|other_rotation|
            {
                let other_id = some_or_return!(name_mappings.world_chunk.get(&(other_rotation, this_name.clone())));

                let (this_rule, other_rule) = {
                    let [a, b] = self.rules.get_disjoint_mut([this_id.0, other_id.0]).unwrap();

                    (a.unwrap(), b.unwrap())
                };

                let difference = other_rotation.subtract(*this_rotation);

                TileRotation::iter().for_each(|neighbor_direction|
                {
                    let output_direction = neighbor_direction.combine(difference);

                    this_rule.neighbors[neighbor_direction.into()].iter().for_each(|neighbor_id|
                    {
                        let (previous_rotation, neighbor_name) = name_mappings.world_chunk.get_back(neighbor_id).unwrap();

                        let new_rotation = previous_rotation.combine(difference);

                        let neighbor = name_mappings.world_chunk.get(&(new_rotation, neighbor_name.clone())).unwrap();

                        let output_direction = output_direction.into();
                        if union(&mut other_rule.neighbors[output_direction], *neighbor)
                        {
                            changed = true;
                            if let Some(track) = other_rule.track.as_ref()
                            {
                                if PosDirection::from(*track) == output_direction
                                {
                                    eprintln!(
                                        "{}: {output_direction} received {} from {} {} by rotational symmetry",
                                        name_mappings.format_id(other_id),
                                        name_mappings.format_id(neighbor),
                                        name_mappings.format_id(this_id),
                                        PosDirection::from(*this_rotation)
                                    );
                                }
                            }
                        }
                    });
                })
            });
        });

        changed
    }

    fn unify_neighbors(
        &mut self,
        name_mappings: &NameMappings,
        ids: &[WorldChunkId]
    )
    {
        ids.iter().for_each(|this_id|
        {
            PosDirection::iter_non_z().for_each(|direction|
            {
                (0..self.rules[this_id.0].neighbors[direction].len()).for_each(|index|
                {
                    let neighbor = self.rules[this_id.0].neighbors[direction][index];
                    let other_rule = &mut self.rules[neighbor.0];

                    let other_direction = direction.opposite();
                    if union(&mut other_rule.neighbors[other_direction], *this_id)
                    {
                        if let Some(track) = other_rule.track.as_ref()
                        {
                            if PosDirection::from(*track) == other_direction
                            {
                                eprintln!(
                                    "{}: {other_direction} received {} from neighbor sharing",
                                    name_mappings.format_id(&neighbor),
                                    name_mappings.format_id(this_id)
                                );
                            }
                        }
                    }
                });
            });
        });
    }

    fn union_neighbors(&mut self, name_mappings: &NameMappings)
    {
        let rules: Vec<_> = self.rules.iter().map(|x| WorldChunkId(x.0)).collect();

        let unify_neighbors = |this: &mut Self|
        {
            this.unify_neighbors(name_mappings, &rules);
        };

        unify_neighbors(self);

        loop
        {
            let self_symmetry_changed = self.self_symmetry_union(name_mappings, &rules);

            if self_symmetry_changed
            {
                unify_neighbors(self);
            }

            let rotation_symmetry_changed = self.rotation_symmetry_union(name_mappings, &rules);

            if rotation_symmetry_changed
            {
                unify_neighbors(self);
            } else
            {
                if !self_symmetry_changed
                {
                    break;
                }
            }
        }
    }

    fn print_neighbors(&self, name_mappings: &NameMappings)
    {
        self.rules.iter().for_each(|(id, rule)|
        {
            eprintln!("{}: {{", name_mappings.format_id(&WorldChunkId(id)));

            rule.neighbors.as_ref().for_each(|direction, ids|
            {
                let rules = ids.iter()
                    .map(|id|
                    {
                        name_mappings.format_id(id)
                    })
                    .reduce(|acc, x|
                    {
                        acc + ", " + &x
                    }).unwrap_or_default();

                eprintln!("    {direction}: [{rules}],");
            });

            eprintln!("}},");
        });
    }

    pub fn generate(&self, id: WorldChunkId) -> WorldChunk
    {
        let rule = self.get(id);

        WorldChunk::new(id, rule.tags.iter().map(|tag|
        {
            WorldChunkTag::generate(tag)
        }).collect())
    }

    pub fn possible_states(&self, difficulty: f32) -> Vec<PossibleState>
    {
        let mut possible_states = self.all_ids.iter().map(|(id, weight)|
        {
            let weight = weight.unwrap_or_else(||
            {
                match &self.rules[id.0].weight
                {
                    ChunkWeight::Lisp(program) => program.eval(|memory|
                    {
                        if let Err(err) = memory.define("difficulty", difficulty.into())
                        {
                            eprintln!("error defining difficulty: {err}");
                        }
                    }).map(|x|
                    {
                        x.as_float().unwrap_or_else(|err|
                        {
                            eprintln!("in `{}` error generating weight: {err}", self.name(*id));

                            0.0
                        }) as f64
                    }).unwrap_or_else(|err|
                    {
                        eprintln!("in `{}` error generating weight: {err}", self.name(*id));

                        0.0
                    }),
                    ChunkWeight::Value(_) => unreachable!()
                }
            });

            PossibleState{id: *id, weight}
        }).collect::<Vec<_>>();

        let total_weight = possible_states.iter().map(|x| x.weight).sum::<f64>();

        possible_states.iter_mut().for_each(|possible_state| possible_state.weight /= total_weight);

        possible_states
    }

    pub fn name(&self, id: WorldChunkId) -> &str
    {
        &self.get(id).name
    }

    pub fn format_id(&self, id: WorldChunkId) -> String
    {
        let rule = self.get(id);
        format_id(&rule.name, rule.rotation)
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
        self.rules.get(id.0)
    }

    pub fn get(&self, id: WorldChunkId) -> &ChunkRule
    {
        self.get_maybe(id).unwrap_or_else(|| panic!("{id} out of range"))
    }

    pub fn iter(&self) -> impl Iterator<Item=&ChunkRule> + '_
    {
        self.rules.iter().map(|x| x.1)
    }
}

impl ParsableRules for ChunkRules
{
    fn parse(
        name_mappings: &mut NameMappings,
        file: File
    ) -> Result<Self, serde_json::Error>
    {
        let rules = serde_json::from_reader::<_, ChunkRulesRaw>(file)?;

        Ok(Self::from_raw(name_mappings, rules))
    }
}
