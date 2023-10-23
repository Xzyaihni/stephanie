use std::{
    fs::File,
    fmt::{self, Debug},
    path::Path,
    collections::HashMap
};

use strum::IntoEnumIterator;

use serde::Deserialize;

use super::{PossibleStates, ParseError};

use crate::common::world::{
    DirectionsGroup,
    chunk::PosDirection
};


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
	neighbors: DirectionsGroup<Vec<usize>>
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

    pub fn neighbors(&self, direction: PosDirection) -> &[usize]
    {
        &self.rule.neighbors[direction]
    }
}

impl<'a> Debug for BorrowedChunkRule<'a>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let neighbors = PosDirection::iter().map(|direction|
        {
            let neighbors = self.neighbors(direction).iter().map(|id|
            {
                format!("\"{}\"", self.rules.get(*id).name())
            }).reduce(|acc, v|
            {
                format!("{acc}, {v}")
            }).unwrap_or_else(|| String::new());

            format!("{direction:?}: {neighbors}")
        }).reduce(|acc, v|
        {
            format!("{acc}, {v}")
        }).unwrap_or_else(|| String::new());

        write!(f, "ChunkRule{{name: \"{}\", neighbors: {{{}}}}}", self.name(), neighbors)
    }
}

#[derive(Debug)]
pub struct ChunkRules
{
    rules: Box<[ChunkRule]>,
    fallback: usize,
    total_weight: f64,
    entropy: f64
}

impl ChunkRules
{
    pub fn load(path: &Path) -> Result<Self, ParseError>
    {
        let json_file = File::open(path).map_err(|err|
        {
            ParseError::new_named(path.to_owned(), err)
        })?;

		let rules = serde_json::from_reader::<_, ChunkRulesRaw>(json_file).map_err(|err|
        {
            ParseError::new_named(path.to_owned(), err)
        })?;

        Ok(Self::from(rules))
    }

    pub fn name(&self, id: usize) -> &str
    {
        &self.rules.get(id).unwrap_or_else(|| panic!("{} out of range", id)).name
    }

    pub fn total_weight(&self) -> f64
    {
        self.total_weight
    }

    pub fn entropy(&self) -> f64
    {
        self.entropy
    }

    pub fn fallback(&self) -> usize
    {
        self.fallback
    }

    pub fn len(&self) -> usize
    {
        self.rules.len()
    }

    pub fn get_maybe(&self, id: usize) -> Option<BorrowedChunkRule<'_>>
    {
        self.rules.get(id).map(|rule|
        {
            BorrowedChunkRule{
                rules: &self,
                rule
            }
        })
    }

    pub fn get(&self, id: usize) -> BorrowedChunkRule<'_>
    {
        self.get_maybe(id).unwrap_or_else(|| panic!("{} out of range", id))
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item=BorrowedChunkRule<'a>> + 'a
    {
        self.rules.iter().skip(1).map(move |rule|
        {
            BorrowedChunkRule{
                rules: &self,
                rule
            }
        })
    }
}

impl From<ChunkRulesRaw> for ChunkRules
{
    fn from(rules: ChunkRulesRaw) -> Self
    {
        let weights = rules.rules.iter().skip(1).map(|rule| rule.weight);

        let total_weight: f64 = weights.clone().sum();
        let entropy = PossibleStates::calculate_entropy(weights);

        let name_mappings = rules.rules.iter().enumerate().map(|(id, rule)|
        {
            (rule.name.clone(), id)
        }).collect::<HashMap<String, usize>>();

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
                let ChunkRuleRaw{
                    name,
                    weight,
                    neighbors
                } = rule;

                ChunkRule{
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
            }).collect::<Box<[_]>>()
        }
    }
}
