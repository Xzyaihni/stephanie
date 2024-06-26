use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

use crate::common::{
    SeededRandom,
    AnyEntities,
    Entity,
    EnemiesInfo,
    EnemyInfo,
    EnemyId,
    Anatomy,
    Physical
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
                    BehaviorState::Wait => 2.0..=5.0,
                    BehaviorState::MoveDirection(_) => 0.5..=1.0,
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
    Attack(Entity)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enemy
{
    behavior: EnemyBehavior,
    behavior_state: BehaviorState,
    current_state_left: Option<f32>,
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
            id,
            rng
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
                        let x = fastrand::f32() * 2.0 - 1.0;
                        let y = fastrand::f32() * 2.0 - 1.0;

                        let direction = Unit::new_normalize(Vector3::new(x, y, 0.0));

                        BehaviorState::MoveDirection(direction)
                    },
                    BehaviorState::MoveDirection(_) => BehaviorState::Wait,
                    BehaviorState::Attack(_) => BehaviorState::Wait
                }
            }
        };

        self.behavior_state = new_state;
    }

    pub fn info<'a>(&self, enemies_info: &'a EnemiesInfo) -> &'a EnemyInfo
    {
        enemies_info.get(self.id)
    }

    fn do_behavior(
        &mut self,
        anatomy: &Anatomy,
        transform: &mut Transform,
        physical: &mut Physical
    )
    {
        let move_speed = match anatomy.speed()
        {
            Some(x) => x / physical.mass,
            None => return
        };

        match &self.behavior_state
        {
            BehaviorState::MoveDirection(direction) =>
            {
                let angle = direction.y.atan2(direction.x);

                physical.velocity = direction.into_inner() * move_speed;
                transform.rotation = angle;
            },
            BehaviorState::Attack(entity) =>
            {
                dbg!(entity);
            },
            BehaviorState::Wait => ()
        }
    }

    pub fn update(
        &mut self,
        entities: &impl AnyEntities,
        entity: Entity,
        dt: f32
    ) -> bool
    {
        let anatomy = entities.anatomy(entity).unwrap();

        if anatomy.speed().is_none()
        {
            return false;
        }

        let changed = if let Some(current_state_left) = self.current_state_left.as_mut()
        {
            *current_state_left -= dt;

            let changed_state = *current_state_left <= 0.0;
            if changed_state
            {
                self.next_state();

                self.current_state_left = self.behavior.duration_of(
                    &mut self.rng,
                    &self.behavior_state
                );
            }

            changed_state
        } else
        {
            false
        };

        let mut transform = entities.target(entity).unwrap();
        let mut physical = entities.physical_mut(entity).unwrap();

        self.do_behavior(&anatomy, &mut transform, &mut physical);

        changed
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
}
