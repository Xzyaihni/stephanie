use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector3};

use crate::{
    debug_config::*,
    common::{
        some_or_value,
        some_or_return,
        angle_between,
        short_rotation,
        character::*,
        raycast::*,
        collider::*,
        entity::{raycast_system, ClientEntities},
        world::{TILE_SIZE, pathfind::*},
        World,
        SeededRandom,
        AnyEntities,
        Entity,
        EnemiesInfo,
        EnemyInfo,
        EnemyId,
        Physical,
        Anatomy
    }
};


pub fn sees(
    entities: &ClientEntities,
    world: &World,
    entity: Entity,
    other_entity: Entity
) -> Option<f32>
{
    let anatomy = entities.anatomy(entity)?;
    let transform = entities.transform(entity)?;

    let other_transform = entities.transform(other_entity)?;
    let other_position = other_transform.position;

    let angle = angle_between(transform.position, other_position);
    let angle_offset = short_rotation(angle + transform.rotation).abs();

    let vision_angle = anatomy.vision_angle().unwrap_or(0.0);

    if angle_offset > vision_angle
    {
        return None;
    }

    let visibility = entities.character(other_entity)?.visibility();

    let vision = anatomy.vision().unwrap_or(0.0);

    let distance = transform.position.metric_distance(&other_position);

    let max_distance = (vision * visibility) + (transform.scale.xy().min() + other_transform.scale.xy().min()) / 2.0;

    let is_visible = distance < max_distance;

    if !is_visible
    {
        return None;
    }

    let info = RaycastInfo{
        pierce: Some(1.0),
        pierce_scale: RaycastPierce::Ignore,
        layer: ColliderLayer::Vision,
        ignore_entity: Some(entity),
        ignore_end: false
    };

    let hits = raycast_system::raycast(
        entities,
        world,
        info,
        &transform.position,
        &other_position
    ).hits;

    let hit_obstacle = hits.into_iter().any(|hit|
    {
        match hit.id
        {
            RaycastHitId::Entity(hit_entity) =>
            {
                let is_target = hit_entity == other_entity;

                let is_friendly = if let (
                    Some(this_character),
                    Some(other_character)
                ) = (entities.character(entity), entities.character(hit_entity))
                {
                    !this_character.aggressive(&other_character)
                } else
                {
                    false
                };

                !is_target && !is_friendly
            },
            RaycastHitId::Tile(pos) =>
            {
                let tile = some_or_value!(world.tile(pos), false);
                !world.tile_info(*tile).transparent
            }
        }
    });

    if hit_obstacle
    {
        return None;
    }

    let angle_fraction = 1.0 - (angle_offset / vision_angle).powi(3);
    let distance_fraction = 1.0 - (distance / max_distance).powi(3);

    Some(visibility * angle_fraction * distance_fraction)
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
    MoveTo(WorldPath),
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
    seen_timer: f32,
    seen_now: bool,
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
            seen_timer: 0.0,
            seen_now: false,
            reset_state: false,
            id,
            rng
        }
    }

    fn next_state(
        &self,
        entities: &impl AnyEntities,
        world: &World,
        this_entity: Entity
    ) -> BehaviorState
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
                        entities.transform(*entity).zip(entities.transform(this_entity).map(|x| x.position))
                            .and_then(|(transform, this_position)|
                            {
                                let path = world.pathfind(this_position, transform.position)?;

                                Some(BehaviorState::MoveTo(path))
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

    fn move_to(
        entities: &ClientEntities,
        entity: Entity,
        point: Vector3<f32>,
        dt: f32
    )
    {
        let transform = some_or_return!(entities.target_ref(entity));

        let distance = point - transform.position;

        if distance.magnitude() < f32::EPSILON
        {
            return;
        }

        let anatomy = entities.anatomy(entity).unwrap();

        let mut physical = some_or_return!(entities.physical_mut_no_change(entity));
        let mut character = some_or_return!(entities.character_mut_no_change(entity));

        Self::move_direction(
            &mut physical,
            &mut character,
            &anatomy,
            Unit::new_normalize(distance),
            dt
        );
    }

    fn do_behavior(
        &mut self,
        entities: &ClientEntities,
        world: &World,
        entity: Entity,
        dt: f32
    )
    {
        let anatomy = entities.anatomy(entity).unwrap();

        if anatomy.speed().is_none()
        {
            return;
        }

        match &mut self.behavior_state
        {
            BehaviorState::MoveDirection(direction) =>
            {
                let mut physical = some_or_return!(entities.physical_mut_no_change(entity));
                let mut character = some_or_return!(entities.character_mut_no_change(entity));

                Self::move_direction(
                    &mut physical,
                    &mut character,
                    &anatomy,
                    *direction,
                    dt
                );
            },
            BehaviorState::MoveTo(path) =>
            {
                if DebugConfig::is_enabled(DebugTool::DisplayPathfind)
                {
                    path.debug_display(entities);
                }

                let transform = some_or_return!(entities.transform(entity));

                let position = transform.position;
                if let Some(direction) = path.move_along(TILE_SIZE * 0.1, position)
                {
                    Self::move_to(entities, entity, position + direction, dt);
                } else
                {
                    self.reset_state = true;
                }
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
                    let aggressive = some_or_return!(entities.character(entity)).aggressive(&other_character);

                    let sees = sees(entities, world, entity, other_entity).is_some();

                    if aggressive && sees
                    {
                        Self::move_to(entities, entity, other_transform.position, dt);

                        let mut character = some_or_return!(entities.character_mut_no_change(entity));

                        let transform = some_or_return!(entities.target_ref(entity));
                        if character.bash_reachable(&transform, &other_transform.position)
                        {
                            character.push_action(CharacterAction::Bash);
                        }

                        return;
                    }
                }

                self.reset_state = true;
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

        if let Some(x) = character.rotation_mut()
        {
            *x = angle;
        }
    }

    pub fn update(
        &mut self,
        entities: &ClientEntities,
        world: &World,
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

        if self.seen_timer > 0.0 && !self.seen_now
        {
            self.seen_timer = (self.seen_timer - dt).max(0.0);
        }

        self.seen_now = false;

        let mut changed = if let Some(current_state_left) = self.current_state_left.as_mut()
        {
            *current_state_left -= dt;

            let changed_state = *current_state_left <= 0.0;
            if changed_state
            {
                self.set_next_state(entities, world, entity);
            }

            changed_state
        } else
        {
            false
        };

        if self.reset_state
        {
            self.reset_state = false;
            self.seen_timer = 0.0;

            changed = true;
            self.set_next_state(entities, world, entity);
        }

        self.do_behavior(entities, world, entity, dt);

        changed
    }

    fn set_next_state(&mut self, entities: &impl AnyEntities, world: &World, entity: Entity)
    {
        self.set_state(self.next_state(entities, world, entity));
    }

    fn set_state(&mut self, state: BehaviorState)
    {
        self.behavior_state = state;

        self.current_state_left = self.behavior.duration_of(
            &mut self.rng,
            &self.behavior_state
        );
    }

    pub fn seen_timer(&self) -> f32
    {
        self.seen_timer
    }

    pub fn seen_fraction(&self) -> Option<f32>
    {
        (self.seen_timer > 0.0).then_some(self.seen_timer)
    }

    pub fn increase_seen(&mut self, dt: f32)
    {
        self.seen_timer += dt;
        self.seen_now = true;
    }

    pub fn set_waiting(&mut self)
    {
        self.set_state(BehaviorState::Wait);
    }

    pub fn set_attacking(&mut self, entity: Entity)
    {
        self.seen_timer = 0.0;
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
        !self.is_attacking() && ((self.hostile_timer <= 0.0) || self.seen_timer > 0.0)
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
