use serde::{Serialize, Deserialize};

use yanyaengine::{
    Transform,
    TransformContainer
};

use crate::{
    entity_forward,
	common::{
        EntityAny,
        EntityAnyWrappable,
        CharacterProperties,
        PhysicalProperties,
		character::Character,
	}
};


pub struct EnemyProperties
{
	pub character_properties: CharacterProperties,
    pub behavior: EnemyBehavior
}

impl EnemyProperties
{
    pub fn physical(&self) -> &PhysicalProperties
    {
        self.character_properties.physical()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnemyBehavior
{
    Melee
}

impl EnemyBehavior
{
    pub fn start_state(&self) -> BehaviorState
    {
        match self
        {
            Self::Melee => BehaviorState::Wait
        }
    }

    pub fn duration_of(&self, state: &BehaviorState) -> f32
    {
        match self
        {
            Self::Melee =>
            {
                match state
                {
                    BehaviorState::Wait => 0.5,
                    BehaviorState::MoveDirection(_) => 1.0
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BehaviorState
{
    Wait,
    MoveDirection(Unit<Vector3<f32>>)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enemy
{
	character: Character,
    behavior: EnemyBehavior,
    behavior_state: BehaviorState
}

impl Enemy
{
	pub fn new(enemy_properties: EnemyProperties) -> Self
	{
		Self{
			character: Character::new(enemy_properties.character_properties),
            behavior_state: enemy_properties.behavior.start_state(),
            behavior: enemy_properties.behavior
		}
	}

    pub fn next_state(&mut self)
    {
        let new_state = match &self.behavior
        {
            EnemyBehavior::Melee =>
            {
                match &self.behavior_state
                {
                    BehaviorState::Wait =>
                    {
                        let x = fastrand::f32();
                        let direction = Unit::new_normalize(Vector3::new(x, 1.0 - x, 0.0));

                        BehaviorState::MoveDirection(direction)
                    },
                    BehaviorState::MoveDirection(_) => BehaviorState::Wait
                }
            }
        };

        self.behavior_state = new_state;
    }

    pub fn behavior(&self) -> &EnemyBehavior
    {
        &self.behavior
    }

    pub fn behavior_state(&self) -> &BehaviorState
    {
        &self.behavior_state
    }

    pub fn set_behavior_state(&mut self, state: BehaviorState)
    {
        self.behavior_state = state;
    }

	pub fn speed(&self) -> Option<f32>
	{
		self.character.speed()
	}

    pub fn set_speed(&mut self, speed: f32)
    {
        self.character.set_speed(speed);
    }
}

impl EntityAnyWrappable for Enemy
{
    fn wrap_any(self) -> EntityAny
    {
        EntityAny::Enemy(self)
    }
}

entity_forward!{Enemy, character}
