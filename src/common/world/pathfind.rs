use std::{
    cell::RefCell,
    ops::ControlFlow,
    cmp::Ordering,
    collections::{HashMap, BinaryHeap}
};

#[allow(unused_imports)]
use std::time::{Instant, Duration};

use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

use crate::{
    debug_config::*,
    common::{
        some_or_return,
        some_or_value,
        line_info,
        watcher::*,
        render_info::*,
        collider::*,
        raycast::{self, *},
        raycast_system::{self, RaycastEntitiesRawInfo},
        Entity,
        AnyEntities,
        EntityInfo,
        World,
        PosDirection,
        Pos3,
        SpatialGrid,
        entity::iterate_components_with,
        world::{
            TILE_SIZE,
            ClientEntities,
            TilePos
        }
    }
};


const PATHFIND_MAX_STEPS: usize = 1000;

fn debug_display_current(entities: &ClientEntities, node: Node)
{
    let v = node.cost * 0.05;
    let color = [v, 0.0, 1.0 - v, 0.5];

    let entity = entities.push(true, EntityInfo{
        transform: Some(Transform{
            position: node.value.center_position().into(),
            scale: Vector3::repeat(TILE_SIZE),
            ..Default::default()
        }),
        render: Some(RenderInfo{
            object: Some(RenderObjectKind::Texture{
                name: "solid.png".into()
            }.into()),
            mix: Some(MixColor{keep_transparency: true, ..MixColor::color(color)}),
            above_world: true,
            ..Default::default()
        }),
        ..Default::default()
    });

    entities.add_watcher(entity, Watcher::simple_disappearing(1.0));
}

fn debug_display_collided_entity(entities: &ClientEntities, entity: Entity, position: TilePos)
{
    let position: Vector3<f32> = position.center_position().into();

    {
        let entity = entities.push(true, EntityInfo{
            transform: Some(Transform{
                position,
                scale: Vector3::repeat(TILE_SIZE),
                ..Default::default()
            }),
            render: Some(RenderInfo{
                object: Some(RenderObjectKind::Texture{
                    name: "solid.png".into()
                }.into()),
                mix: Some(MixColor{keep_transparency: true, ..MixColor::color([1.0, 1.0, 0.0, 0.5])}),
                above_world: true,
                ..Default::default()
            }),
            ..Default::default()
        });

        entities.add_watcher(entity, Watcher::simple_disappearing(1.0));
    }

    let other_position = some_or_return!(entities.transform(entity)).position;
    if let Some(line) = line_info(position, other_position, 0.005, [0.0, 1.0, 1.0])
    {
        let entity = entities.push(true, line);
        entities.add_watcher(entity, Watcher::simple_one_frame());
    }
}

#[cfg(debug_assertions)]
struct DebugTimer
{
    current: Instant,
    inside_count: usize,
    inside_spatial: Duration,
    outside_count: usize,
    outside_spatial: Duration
}

#[cfg(not(debug_assertions))]
struct DebugTimer;

#[cfg(debug_assertions)]
impl DebugTimer
{
    fn new() -> Self
    {
        Self{
            current: Instant::now(),
            inside_count: 0,
            inside_spatial: Duration::default(),
            outside_count: 0,
            outside_spatial: Duration::default()
        }
    }

    fn start(&mut self)
    {
        self.current = Instant::now();
    }

    fn end_with(&mut self, state: bool)
    {
        let passed = self.current.elapsed();

        if state
        {
            self.inside_count += 1;
            self.inside_spatial += passed;
        } else
        {
            self.outside_count += 1;
            self.outside_spatial += passed;
        }
    }

    fn print(&self)
    {
        if DebugConfig::is_disabled(DebugTool::DebugTimings)
        {
            return;
        }

        fn f(name: &str, count: usize, time: Duration)
        {
            if count != 0
            {
                let time_us = time.as_micros() as f64;
                let per_run = time_us / count as f64;

                eprintln!("{name} called {count} times, total {:.2} ms ({per_run:.2} us per run)", time_us / 1000.0);
            } else
            {
                eprintln!("{name} never called");
            }
        }

        f("inside", self.inside_count, self.inside_spatial);
        f("outside", self.outside_count, self.outside_spatial);
    }
}

#[cfg(not(debug_assertions))]
impl DebugTimer
{
    fn new() -> Self { Self }

    fn start(&self) {}
    fn end_with(&self, _state: bool) {}

    fn print(&self) {}
}

#[derive(Clone, Copy)]
pub struct Pathfinder<'a>
{
    pub world: &'a World,
    pub entities: &'a ClientEntities,
    pub space: &'a SpatialGrid
}

impl Pathfinder<'_>
{
    pub fn pathfind(
        &self,
        entity: Entity,
        start: Vector3<f32>,
        end: Vector3<f32>
    ) -> Option<WorldPath>
    {
        let layer = self.pathfind_layer(entity);

        let scale = self.entities.collider(entity)
            .and_then(|x| x.override_transform.as_ref().map(|x| x.transform.scale))
            .or_else(|| self.entities.transform(entity).map(|x| x.scale))
            .unwrap_or_else(Vector3::zeros);

        let direction = end - start;

        if self.straight_line_free(entity, start, direction, scale, layer)
        {
            return Some(WorldPath::new(vec![end, start]));
        }

        crate::debug_time_this!{"pathfind-full", self.pathfind_full(entity, layer, scale, start, end)}
    }

    fn pathfind_full(
        &self,
        entity: Entity,
        layer: Option<ColliderLayer>,
        scale: Vector3<f32>,
        start: Vector3<f32>,
        end: Vector3<f32>
    ) -> Option<WorldPath>
    {
        let mut debug_timer = DebugTimer::new();

        let tile_colliding = |pos|
        {
            self.world.tile(pos).map(|x| self.world.tile_info(*x).colliding).unwrap_or(true)
        };

        let target = TilePos::from(end);
        let start = TilePos::from(start);

        let mut steps = 0;

        let mut unexplored = BinaryHeap::from([
            Node{cost: 0.0, value: start}
        ]);

        let mut explored = HashMap::from([(start, NodeInfo{moves_from_start: 0, previous: None})]);

        while !unexplored.is_empty()
        {
            steps += 1;
            if steps > PATHFIND_MAX_STEPS
            {
                return None;
            }

            let current = unexplored.pop()?;

            if DebugConfig::is_enabled(DebugTool::DisplayPathfindAttempt)
            {
                debug_display_current(self.entities, current.clone());
            }

            if current.value == target
            {
                let current_position: Vector3<f32> = current.value.center_position().into();
                let mut path = vec![Vector3::new(end.x, end.y, current_position.z), current_position];
                current.path_to(&mut explored, &mut path, |x| x.center_position().into());

                debug_timer.print();
                return Some(crate::debug_time_this!{"simplify-path", self.simplify_path(entity, scale, layer, path)});
            }

            let below = current.value.offset(Pos3::new(0, 0, -1));
            let is_grounded = tile_colliding(below);

            let mut try_push = |position: TilePos|
            {
                let moves_from_start = explored[&current.value].moves_from_start;

                if let Some(explored) = explored.get_mut(&position)
                {
                    if explored.moves_from_start > moves_from_start + 1
                    {
                        explored.moves_from_start = moves_from_start + 1;
                        explored.previous = Some(current.clone());
                    }
                } else
                {
                    let moves_from_start = moves_from_start + 1;

                    let info = NodeInfo{moves_from_start, previous: Some(current.clone())};
                    explored.insert(position, info);

                    let goal_distance = Vector3::from(position.distance(target)).cast::<f32>().magnitude();

                    let cost = moves_from_start as f32 + goal_distance;

                    unexplored.push(Node{
                        cost,
                        value: position
                    });
                }
            };

            if is_grounded
            {
                PosDirection::iter_non_z().for_each(|direction|
                {
                    let position = current.value.offset(Pos3::from(direction));

                    let is_colliding_entity = |debug_timer|
                    {
                        let layer = some_or_value!(layer, false);

                        self.is_colliding_entity(entity, layer, scale, position, debug_timer)
                    };

                    if (position == target)
                        || (!tile_colliding(position) && !is_colliding_entity(&mut debug_timer))
                    {
                        try_push(position);
                    }
                });
            } else
            {
                try_push(below);
            }
        }

        debug_timer.print();
        None
    }

    fn is_colliding_entity(
        &self,
        check_entity: Entity,
        layer: ColliderLayer,
        scale: Vector3<f32>,
        position: TilePos,
        debug_timer: &mut DebugTimer
    ) -> bool
    {
        let center_position = position.center_position().into();

        let tile_checker = ColliderInfo{
            kind: ColliderType::Circle,
            layer,
            ghost: true,
            sleeping: false,
            override_transform: None
        }.into();

        let tile_checker = {
            let transform = Transform{
                position: center_position,
                scale,
                ..Default::default()
            };

            CollidingInfoRef::new(transform, &tile_checker)
        };

        let is_colliding = |this_collider: &Collider, entity|
        {
            if entity == check_entity
            {
                return ControlFlow::Continue(());
            }

            if this_collider.ghost
            {
                return ControlFlow::Continue(());
            }

            let this_transform = some_or_value!(self.entities.transform(entity), ControlFlow::Continue(()));

            let this = CollidingInfoRef::new(this_transform.clone(), &this_collider);

            let is_colliding = this.collide_immutable(&tile_checker, |_| {});

            if is_colliding
            {
                if DebugConfig::is_enabled(DebugTool::DisplayPathfindAttempt)
                {
                    debug_display_collided_entity(self.entities, entity, position);
                }

                ControlFlow::Break(())
            } else
            {
                ControlFlow::Continue(())
            }
        };

        let inside_simulated = self.space.inside_simulated(center_position, TILE_SIZE.hypot(TILE_SIZE));

        debug_timer.start();

        let control = if inside_simulated
        {
            self.space.try_for_each_near(position, |entity|
            {
                let this_collider = some_or_value!(self.entities.collider(entity), ControlFlow::Continue(()));

                is_colliding(&this_collider, entity)
            })
        } else
        {
            iterate_components_with!(&self.entities, collider, try_for_each, |entity, collider: &RefCell<Collider>|
            {
                is_colliding(&collider.borrow(), entity)
            })
        };

        debug_timer.end_with(inside_simulated);

        control.is_break()
    }

    fn simplify_path(
        &self,
        entity: Entity,
        scale: Vector3<f32>,
        layer: Option<ColliderLayer>,
        tiles: Vec<Vector3<f32>>
    ) -> WorldPath
    {
        let mut check = 0;

        let mut simplified = vec![tiles[0]];

        let mut index = 1;
        while index < tiles.len()
        {
            let is_next = (check + 1) == index;

            let is_tile_reachable = |tiles: &[Vector3<f32>]|
            {
                let distance = tiles[index] - tiles[check];

                let start = Vector3::from(tiles[check]);

                self.straight_line_free(entity, start, distance, scale, layer)
            };

            let is_reachable = is_next || is_tile_reachable(&tiles);

            if is_reachable
            {
                index += 1;
            } else
            {
                check = index - 1;

                simplified.push(tiles[check]);
            }
        }

        WorldPath::new(simplified)
    }

    fn straight_line_free(
        &self,
        entity: Entity,
        start: Vector3<f32>,
        direction: Vector3<f32>,
        scale: Vector3<f32>,
        layer: Option<ColliderLayer>
    ) -> bool
    {
        let collides_world = raycast::swept_aabb_world_collides(
            self.world,
            &Transform{
                position: start,
                scale,
                ..Default::default()
            },
            direction
        );

        let collides_entities = ||
        {
            let layer = some_or_value!(layer, false);

            let end = start + direction;

            let max_distance = direction.magnitude();

            let direction = Unit::new_unchecked(direction / max_distance);

            raycast_system::raycast_entities_any_raw(
                self.space,
                scale.y.hypot(scale.x),
                end,
                raycast_system::before_raycast_default(layer, Some(entity)),
                RaycastEntitiesRawInfo{
                    entities: self.entities,
                    start,
                    direction,
                    after_raycast: raycast_system::after_raycast_default(max_distance, false),
                    raycast_fn: |start, direction, kind, transform: &Transform|
                    {
                        raycast_this(start, direction, kind, &Transform{
                            scale: transform.scale + scale,
                            ..transform.clone()
                        })
                    }
                }
            )
        };

        !collides_world && !collides_entities()
    }

    fn pathfind_layer(&self, entity: Entity) -> Option<ColliderLayer>
    {
        if self.entities.enemy_exists(entity)
        {
            Some(ColliderLayer::PathfindEnemy)
        } else
        {
            self.entities.collider(entity).map(|x| x.layer)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPath
{
    values: Vec<Vector3<f32>>
}

impl WorldPath
{
    pub fn new(values: Vec<Vector3<f32>>) -> Self
    {
        Self{values}
    }

    pub fn target(&self) -> Option<&Vector3<f32>>
    {
        self.values.first()
    }

    pub fn remove_current_target(&mut self)
    {
        self.values.pop();
    }

    pub fn move_along(
        &mut self,
        near: f32,
        position: Vector3<f32>
    ) -> Option<Vector3<f32>>
    {
        if self.values.is_empty()
        {
            return None;
        }

        let target_position = self.values.last().unwrap();

        let distance = target_position - position;

        if distance.magnitude() < near
        {
            self.remove_current_target();
            return self.move_along(near, position)
        }

        Some(distance)
    }

    pub fn debug_display(&self, entities: &ClientEntities)
    {
        let amount = self.values.len();
        self.values.iter().copied().enumerate().for_each(|(index, position)|
        {
            let is_selected = (index + 1) == amount;

            let color = if is_selected
            {
                [1.0, 0.0, 0.0, 0.5]
            } else
            {
                [0.0, 0.0, 1.0, 0.5]
            };

            let entity = entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position,
                    scale: Vector3::repeat(TILE_SIZE * 0.3),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "circle.png".into()
                    }.into()),
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color(color)}),
                    above_world: true,
                    ..Default::default()
                }),
                ..Default::default()
            });

            entities.add_watcher(entity, Watcher::simple_one_frame());
        });

        self.values.iter().zip(self.values.iter().skip(1)).for_each(|(previous, current)|
        {
            if let Some(info) = line_info(*previous, *current, TILE_SIZE * 0.1, [0.2, 0.2, 1.0])
            {
                let entity = entities.push(true, info);
                entities.add_watcher(entity, Watcher::simple_one_frame());
            }
        });
    }
}

struct NodeInfo
{
    moves_from_start: u32,
    previous: Option<Node>
}

#[derive(Debug, Clone)]
struct Node
{
    cost: f32,
    value: TilePos
}

impl Node
{
    fn path_to<T, F: Fn(TilePos) -> T>(
        self,
        explored: &mut HashMap<TilePos, NodeInfo>,
        path: &mut Vec<T>,
        f: F
    )
    {
        if let Some(node) = explored.remove(&self.value).unwrap().previous
        {
            path.push(f(node.value));
            node.path_to(explored, path, f);
        }
    }
}

impl PartialEq for Node
{
    fn eq(&self, other: &Self) -> bool
    {
        self.cost.eq(&other.cost)
    }
}

impl Eq for Node {}

impl PartialOrd for Node
{
    // clippy bug
    #[allow(clippy::non_canonical_partial_ord_impl)]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering>
    {
        other.cost.partial_cmp(&self.cost)
    }
}

impl Ord for Node
{
    fn cmp(&self, other: &Self) -> Ordering { self.partial_cmp(other).unwrap_or(Ordering::Equal) }
}
