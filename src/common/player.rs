use serde::{Serialize, Deserialize};

use strum::{EnumIter, EnumCount};

use crate::common::{Entity, Pos3};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnConnectInfo
{
    pub player_entity: Entity,
    pub player_position: Pos3<f32>,
    pub time: f64
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StatLevel
{
    level: u32,
    experience: f64
}

impl Default for StatLevel
{
    fn default() -> Self
    {
        Self{level: 0, experience: 0.0}
    }
}

impl StatLevel
{
    fn try_level_up(&mut self) -> bool
    {
        let goal = self.experience_goal();
        if self.experience >= goal
        {
            self.experience -= goal;
            self.level += 1;

            self.try_level_up();

            true
        } else
        {
            false
        }
    }

    pub fn level(&self) -> u32
    {
        self.level
    }

    pub fn add_experience(&mut self, amount: f64) -> bool
    {
        self.experience += amount;

        self.try_level_up()
    }

    fn experience_goal(&self) -> f64
    {
        (self.level + 1) as f64 * 10.0
    }

    pub fn progress(&self) -> f32
    {
        (self.experience / self.experience_goal()) as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumCount, Serialize, Deserialize)]
pub enum StatId
{
    Melee = 0,
    Bash,
    Poke,
    Throw,
    Ranged,
    Crafting
}

impl StatId
{
    pub fn name(&self) -> &'static str
    {
        match self
        {
            Self::Melee => "melee",
            Self::Bash => "bash",
            Self::Poke => "poke",
            Self::Throw => "throw",
            Self::Ranged => "ranged",
            Self::Crafting => "crafting"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
    pub kills: u32,
    pub stats: [StatLevel; StatId::COUNT]
}

impl Default for Player
{
    fn default() -> Self
    {
        Self{kills: 0, stats: [StatLevel::default(); StatId::COUNT]}
    }
}

impl Player
{
    pub fn get_stat(&self, id: StatId) -> &StatLevel
    {
        &self.stats[id as usize]
    }

    pub fn get_stat_mut(&mut self, id: StatId) -> &mut StatLevel
    {
        &mut self.stats[id as usize]
    }
}
