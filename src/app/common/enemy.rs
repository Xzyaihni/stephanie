use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector3};

use crate::common::{
    some_or_value,
    some_or_return,
    character::*,
    SeededRandom,
    AnyEntities,
    Entity,
    EnemiesInfo,
    EnemyInfo,
    EnemyId,
    Physical,
    Anatomy
};


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

    pub fn duration_of(&self, rng: &mut SeededRandom, state: &BehaviorState) -> Option<f32>
    {
        let range = match self
        {
            Self::Melee =>
            {
                match state
                {
                    BehaviorState::Wait => 10.0..=20.0,
                    BehaviorState::MoveDirection(_) => 0.8..=2.0,
                    BehaviorState::MoveTo(_) => return None,
                    BehaviorState::Attack(_) => return None
                }
            }
        };

        Some(rng.next_f32_between(range))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BehaviorState
{
    Wait,
    MoveDirection(Unit<Vector3<f32>>),
    MoveTo(Vector3<f32>),
    Attack(Entity)
}

impl Default for BehaviorState
{
    fn default() -> Self
    {
        Self::Wait
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enemy
{
    behavior: EnemyBehavior,
    behavior_state: BehaviorState,
    current_state_left: Option<f32>,
    hostile_timer: f32,
    reset_state: bool,
    id: EnemyId,
    rng: SeededRandom
}

impl Enemy
{
    pub fn new(enemies_info: &EnemiesInfo, id: EnemyId) -> Self
    {
        let behavior = enemies_info.get(id).behavior.clone();

        let mut rng = SeededRandom::new();
        let behavior_state = behavior.start_state();

        Self{
            current_state_left: behavior.duration_of(&mut rng, &behavior_state),
            behavior_state,
            behavior,
            hostile_timer: 0.0,
            reset_state: false,
            id,
            rng
        }
    }

    fn next_state(&self, entities: &impl AnyEntities) -> BehaviorState
    {
        match &self.behavior
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
                    BehaviorState::MoveDirection(_) => BehaviorState::Wait,
                    BehaviorState::MoveTo(_) => BehaviorState::Wait,
                    BehaviorState::Attack(entity) =>
                    {
                        entities.transform(*entity).map(|transform|
                        {
                            BehaviorState::MoveTo(transform.position)
                        }).unwrap_or_else(|| BehaviorState::Wait)
                    }
                }
            }
        }
    }

    pub fn info<'a>(&self, enemies_info: &'a EnemiesInfo) -> &'a EnemyInfo
    {
        enemies_info.get(self.id)
    }

    fn do_behavior(
        &mut self,
        entities: &impl AnyEntities,
        entity: Entity,
        dt: f32
    )
    {
        let anatomy = entities.anatomy(entity).unwrap();

        if anatomy.speed().is_none()
        {
            return;
        }

        let transform = some_or_return!(entities.target_ref(entity));
        let mut physical = some_or_return!(entities.physical_mut(entity));
        let mut character = some_or_return!(entities.character_mut(entity));

        match &self.behavior_state
        {
            BehaviorState::MoveDirection(direction) =>
            {
                Self::move_direction(
                    &mut physical,
                    &mut character,
                    &anatomy,
                    *direction,
                    dt
                );
            },
            BehaviorState::MoveTo(point) =>
            {
                let distance = point - transform.position;

                if distance.magnitude() < transform.scale.min()
                {
                    self.reset_state = true;
                    return;
                }

                Self::move_direction(
                    &mut physical,
                    &mut character,
                    &anatomy,
                    Unit::new_normalize(distance),
                    dt
                );
            },
            BehaviorState::Attack(other_entity) =>
            {
                let other_entity = *other_entity;

                if entity == other_entity
                {
                    self.reset_state = true;
                    return;
                }

                if let Some(other_transform) = entities.transform(other_entity)
                {
                    let other_character = entities.character(other_entity).unwrap();
                    let aggressive = character.aggressive(
                        &other_character
                    );

                    let sees = anatomy.sees(&transform, other_character.visibility(), &other_transform.position);

                    if aggressive && sees
                    {
                        let direction = other_transform.position - transform.position;

                        Self::move_direction(
                            &mut physical,
                            &mut character,
                            &anatomy,
                            some_or_return!(Unit::try_new(direction, 0.01)),
                            dt
                        );

                        if character.bash_reachable(&transform, &other_transform.position)
                        {
                            character.push_action(CharacterAction::Bash);
                        }
                    } else
                    {
                        self.reset_state = true;
                    }
                } else
                {
                    self.reset_state = true;
                }
            },
            BehaviorState::Wait => ()
        }
    }

    fn move_direction(
        physical: &mut Physical,
        character: &mut Character,
        anatomy: &Anatomy,
        direction: Unit<Vector3<f32>>,
        dt: f32
    )
    {
        Self::look_direction(character, direction);

        character.walk(anatomy, physical, direction, dt);
    }

    fn look_direction(
        character: &mut Character,
        direction: Unit<Vector3<f32>>
    )
    {
        let angle = direction.y.atan2(direction.x);

        character.rotation = angle;
    }

    pub fn update(
        &mut self,
        entities: &impl AnyEntities,
        entity: Entity,
        dt: f32
    ) -> bool
    {
        let anatomy = some_or_value!{entities.anatomy(entity), false};

        if anatomy.speed().is_none()
        {
            return false;
        }

        if self.hostile_timer <= 0.0
        {
            self.hostile_timer = 0.5;
        } else
        {
            self.hostile_timer -= dt;
        }

        let mut changed = if let Some(current_state_left) = self.current_state_left.as_mut()
        {
            *current_state_left -= dt;

            let changed_state = *current_state_left <= 0.0;
            if changed_state
            {
                self.set_next_state(entities);
            }

            changed_state
        } else
        {
            false
        };

        if self.reset_state
        {
            self.reset_state = false;

            changed = true;
            self.set_next_state(entities);
        }

        self.do_behavior(entities, entity, dt);

        changed
    }

    fn set_next_state(&mut self, entities: &impl AnyEntities)
    {
        self.set_state(self.next_state(entities));
    }

    fn set_state(&mut self, state: BehaviorState)
    {
        self.behavior_state = state;

        self.current_state_left = self.behavior.duration_of(
            &mut self.rng,
            &self.behavior_state
        );
    }

    pub fn set_attacking(&mut self, entity: Entity)
    {
        self.set_state(BehaviorState::Attack(entity));
    }

    pub fn is_attacking(&self) -> bool
    {
        match self.behavior_state
        {
            BehaviorState::Attack(_) => true,
            _ => false
        }
    }

    pub fn check_hostiles(&self) -> bool
    {
        !self.is_attacking() && (self.hostile_timer <= 0.0)
    }

    pub fn behavior(&self) -> &EnemyBehavior
    {
        &self.behavior
    }

    pub fn behavior_state(&self) -> &BehaviorState
    {
        &self.behavior_state
    }
}
