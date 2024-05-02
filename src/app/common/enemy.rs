use serde::{Serialize, Deserialize};

use crate::{
    entity_forward_transform,
    entity_forward_parent,
    entity_forward_drawable,
    forward_damageable,
	common::{
        SeededRandom,
        EntityAny,
        EntityAnyWrappable,
        CharacterProperties,
        EntityProperties,
        PhysicalProperties,
        Physical,
        ChildEntity,
        entity::child_entity::*,
        physics::PhysicsEntity,
		character::Character
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

    pub fn duration_of(&self, rng: &mut SeededRandom, state: &BehaviorState) -> f32
    {
        match self
        {
            Self::Melee =>
            {
                match state
                {
                    BehaviorState::Wait => rng.next_f32_between(2.0..=5.0),
                    BehaviorState::MoveDirection(_) => rng.next_f32_between(0.5..=1.0)
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
    behavior_state: BehaviorState,
    current_state_left: f32,
    rng: SeededRandom
}

impl Enemy
{
	pub fn new(enemy_properties: EnemyProperties) -> Self
	{
        let mut rng = SeededRandom::new();
        let behavior_state = enemy_properties.behavior.start_state();

        let character_properties = enemy_properties.character_properties;

        let entity_properties = character_properties.entity_properties;

        let props = CharacterProperties{
            entity_properties: EntityProperties{
                texture: None,
                ..entity_properties.clone()
            },
            ..character_properties
        };

		let mut this = Self{
			character: Character::new(props),
            current_state_left: enemy_properties.behavior.duration_of(&mut rng, &behavior_state),
            behavior_state,
            behavior: enemy_properties.behavior,
            rng
		};

        let texture = entity_properties.texture;
        let physical = PhysicalProperties{
            transform: Transform{
                position: Vector3::zeros(),
                rotation: 0.0,
                ..entity_properties.physical.transform
            },
            ..entity_properties.physical
        };

        let entity = ChildEntity::new(
            ChildConnection::Rigid,
            ChildRotation::EaseOut(EaseOutRotation{resistance: 0.01, momentum: 0.0}.into()),
            ChildDeformation::Rigid,
            Entity::new(EntityProperties{texture, physical}),
            0
        );

        this.add_child(Vector3::zeros(), entity);

        this
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
                        let x = fastrand::f32() * 2.0 - 1.0;
                        let y = fastrand::f32() * 2.0 - 1.0;

                        let direction = Unit::new_normalize(Vector3::new(x, y, 0.0));

                        BehaviorState::MoveDirection(direction)
                    },
                    BehaviorState::MoveDirection(_) => BehaviorState::Wait
                }
            }
        };

        self.behavior_state = new_state;
    }

    fn do_behavior(&mut self)
    {
        let move_speed = match self.move_speed()
        {
            Some(x) => x,
            None => return
        };

        match &self.behavior_state
        {
            BehaviorState::MoveDirection(direction) =>
            {
                let angle = direction.y.atan2(direction.x);

                self.set_velocity(direction.into_inner() * move_speed);
                self.set_rotation(angle);
            },
            BehaviorState::Wait => ()
        }
    }

    pub fn update(&mut self, dt: f32) -> bool
    {
        self.current_state_left -= dt;

        let needs_update = self.current_state_left <= 0.0;

        if needs_update
        {
            self.next_state();

            self.current_state_left = self.behavior.duration_of(
                &mut self.rng,
                &self.behavior_state
            );
        }

        self.do_behavior();

        self.character.physics_update(dt);

        needs_update
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

    pub fn move_speed(&self) -> Option<f32>
    {
        self.character.move_speed()
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

forward_damageable!{Enemy, character}
entity_forward_parent!{Enemy, character}
entity_forward_transform!{Enemy, character}
entity_forward_drawable!{Enemy, character}

impl PhysicsEntity for Enemy
{
    fn physical_ref(&self) -> &Physical
    {
        self.character.physical_ref()
    }

    fn physical_mut(&mut self) -> &mut Physical
    {
        self.character.physical_mut()
    }

    fn physics_update(&mut self, dt: f32)
    {
        self.update(dt);
    }
}
