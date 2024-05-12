use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

use crate::common::{SeededRandom, Anatomy, Physical};


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

pub struct EnemyProperties
{
    pub behavior: EnemyBehavior
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enemy
{
    behavior: EnemyBehavior,
    behavior_state: BehaviorState,
    current_state_left: f32,
    rng: SeededRandom
}

impl From<EnemyProperties> for Enemy
{
    fn from(properties: EnemyProperties) -> Self
    {
        let mut rng = SeededRandom::new();
        let behavior_state = properties.behavior.start_state();

		Self{
            current_state_left: properties.behavior.duration_of(&mut rng, &behavior_state),
            behavior_state,
            behavior: properties.behavior,
            rng
		}
    }
}

impl Enemy
{
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
            BehaviorState::Wait => ()
        }
    }

    pub fn update(
        &mut self,
        anatomy: &Anatomy,
        transform: &mut Transform,
        physical: &mut Physical,
        dt: f32
    ) -> bool
    {
        self.current_state_left -= dt;

        let changed_state = self.current_state_left <= 0.0;

        if changed_state
        {
            self.next_state();

            self.current_state_left = self.behavior.duration_of(
                &mut self.rng,
                &self.behavior_state
            );
        }

        self.do_behavior(anatomy, transform, physical);

        changed_state
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
