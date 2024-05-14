use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector3};

use yanyaengine::{Transform, game_object::*};

use crate::common::{
    SeededRandom,
    ClientRenderInfo,
    EnemiesInfo,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpriteState
{
    Normal,
    Lying
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Stateful<T>
{
    changed: bool,
    value: T
}

impl<T> From<T> for Stateful<T>
{
    fn from(value: T) -> Self
    {
        Self{
            changed: true,
            value
        }
    }
}

impl<T> Stateful<T>
{
    pub fn set_state(&mut self, value: T)
    where
        T: PartialEq
    {
        if self.value != value
        {
            self.value = value;
            self.changed = true;
        }
    }

    pub fn value(&self) -> &T
    {
        &self.value
    }

    pub fn changed(&mut self) -> bool
    {
        let state = self.changed;

        self.changed = false;

        state
    }
}

pub struct ClientInfo
{
    sprite_state: Stateful<SpriteState>
}

impl Default for ClientInfo
{
    fn default() -> Self
    {
        Self{
            sprite_state: SpriteState::Normal.into()
        }
    }
}

pub type ServerEnemy = Enemy<()>;
pub type ClientEnemy = Enemy<ClientInfo>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enemy<Info>
{
    behavior: EnemyBehavior,
    behavior_state: BehaviorState,
    current_state_left: f32,
    id: EnemyId,
    rng: SeededRandom,
    info: Info
}

impl From<ServerEnemy> for ClientEnemy
{
    fn from(enemy: ServerEnemy) -> Self
    {
        Self{
            behavior: enemy.behavior,
            behavior_state: enemy.behavior_state,
            current_state_left: enemy.current_state_left,
            id: enemy.id,
            rng: enemy.rng,
            info: ClientInfo::default()
        }
    }
}

impl ServerEnemy
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
            rng,
            info: ()
        }
    }

}

impl<Info> Enemy<Info>
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

impl ClientEnemy
{
    pub fn with_previous(&mut self, previous: Self)
    {
        self.info = previous.info;
    }

    pub fn update_sprite(
        &mut self,
        create_info: &mut ObjectCreateInfo,
        transform: Option<&Transform>,
        enemies_info: &EnemiesInfo,
        render: &mut ClientRenderInfo
    )
    {
        if !self.info.sprite_state.changed()
        {
            return;
        }

        let info = enemies_info.get(self.id);
        let texture = match self.info.sprite_state.value()
        {
            SpriteState::Normal => info.normal,
            SpriteState::Lying => info.lying
        };

        render.set_sprite(create_info, transform, texture);
    }

    pub fn set_sprite(&mut self, state: SpriteState)
    {
        self.info.sprite_state.set_state(state);
    }
}
