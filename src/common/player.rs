use std::mem;

use nalgebra::Vector2;

use serde::{Serialize, Deserialize};

use strum::{EnumIter, EnumCount};

use crate::common::{Entity, Pos3};


pub const WEAKER_SCREENSHAKE: f32 = 0.02;
pub const WEAK_SCREENSHAKE: f32 = 0.05;
pub const MEDIUM_SCREENSHAKE: f32 = 0.1;
pub const STRONG_SCREENSHAKE: f32 = 0.2;

pub const WEAK_KICK: f32 = 0.04;
pub const MEDIUM_KICK: f32 = 0.1;

const OFFSET_MAX: f32 = 0.2;

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

    fn add_experience(&mut self, amount: f64) -> bool
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

#[derive(EnumCount)]
pub enum ScreenshakeName
{
    Normal = 0,
    Shot
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct ScreenshakeLayer
{
    duration: f32,
    amount: f32,
}

impl Default for ScreenshakeLayer
{
    fn default() -> Self
    {
        Self{
            duration: 0.0,
            amount: 0.0
        }
    }
}

impl ScreenshakeLayer
{
    pub fn effective_shake(&self) -> f32
    {
        let falloff = (self.duration / 0.1).min(1.0);

        falloff * self.amount
    }

    pub fn set(&mut self, amount: f32)
    {
        if self.effective_shake() < amount
        {
            self.duration = 0.2;
            self.amount = amount;
        }
    }

    pub fn update(&mut self, dt: f32)
    {
        self.duration = (self.duration - dt).max(0.0);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Screenshake
{
    layers: [ScreenshakeLayer; ScreenshakeName::COUNT],
    offset: Vector2<f32>
}

impl Default for Screenshake
{
    fn default() -> Self
    {
        Self{
            layers: [ScreenshakeLayer::default(); ScreenshakeName::COUNT],
            offset: Vector2::zeros()
        }
    }
}

impl Screenshake
{
    pub fn effective_shake(&self) -> f32
    {
        self.layers.iter().map(ScreenshakeLayer::effective_shake).sum()
    }

    pub fn set(&mut self, amount: f32)
    {
        self.set_layer(ScreenshakeName::Normal, amount);
    }

    pub fn set_layer(&mut self, layer: ScreenshakeName, amount: f32)
    {
        self.layers[layer as usize].set(amount);
    }

    pub fn add_offset(&mut self, offset: Vector2<f32>)
    {
        let new_offset = self.offset + offset;

        if new_offset.magnitude() <= OFFSET_MAX
        {
            self.offset = new_offset;
        } else
        {
            self.offset = new_offset.normalize() * OFFSET_MAX;
        }
    }

    pub fn offset(&self) -> Vector2<f32>
    {
        self.offset
    }

    pub fn update(&mut self, dt: f32)
    {
        self.layers.iter_mut().for_each(|layer| layer.update(dt));

        self.offset *= 0.01_f32.powf(dt);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player
{
    #[serde(skip, default)]
    pub screenshake: Screenshake,
    pub kills: u32,
    pub stats: [StatLevel; StatId::COUNT],
    leveled_up: Vec<StatId>
}

impl Default for Player
{
    fn default() -> Self
    {
        Self{
            screenshake: Screenshake::default(),
            kills: 0,
            stats: [StatLevel::default(); StatId::COUNT],
            leveled_up: Vec::new()
        }
    }
}

impl Player
{
    pub fn get_stat(&self, id: StatId) -> &StatLevel
    {
        &self.stats[id as usize]
    }

    pub fn add_experience(&mut self, id: StatId, amount: f64)
    {
        if self.stats[id as usize].add_experience(amount)
        {
            self.leveled_up.push(id);
        }
    }

    pub fn take_leveled_up(&mut self) -> Option<Vec<StatId>>
    {
        if self.leveled_up.is_empty()
        {
            return None;
        }

        Some(mem::take(&mut self.leveled_up))
    }
}
