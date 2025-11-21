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
        SpecialTile,
        entity::iterate_components_with,
        world::{
            TILE_SIZE,
            ClientEntities,
            TilePos
        }
    }
};


const PATHFIND_MAX_STEPS: usize = 1000;
const PATHFIND_POINTS_LIMIT: u32 = 100;

fn debug_display_current(entities: &ClientEntities, node: Node)
{
    let v = node.cost * 0.05;
    let color = [v, 0.0, 1.0 - v, 0.5];

    let name = match node.value.kind
    {
        NodeKind::MoveTo => "solid.png",
        NodeKind::StairsMove{down} => if down { "ui/down_icon.png" } else { "ui/up_icon.png" },
        NodeKind::BreakTile => "ui/close_button.png"
    };

    let entity = entities.push(true, EntityInfo{
        transform: Some(Transform{
            position: node.value.position.center_position().into(),
            scale: Vector3::repeat(TILE_SIZE),
            ..Default::default()
        }),
        render: Some(RenderInfo{
            object: Some(RenderObjectKind::Texture{
                name: name.into()
            }.into()),
            mix: Some(MixColor::color(color)),
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
                mix: Some(MixColor::color([1.0, 1.0, 0.0, 0.5])),
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

struct PathfindLimits(u32);

impl PathfindLimits
{
    fn add(&mut self, inside: bool)
    {
        let value = if inside { 1 } else { 10 };

        self.0 += value;
    }

    fn over(&self) -> bool
    {
        self.0 > PATHFIND_POINTS_LIMIT
    }
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
    pub fn pathfind_straight(
        &self,
        entity: Entity,
        start: Vector3<f32>,
        end: Vector3<f32>
    ) -> Option<WorldPath>
    {
        let layer = self.pathfind_layer(entity);
        let scale = self.pathfind_scale(entity);

        self.pathfind_straight_inner(entity, layer, scale, start, end)
    }

    fn pathfind_straight_inner(
        &self,
        entity: Entity,
        layer: Option<ColliderLayer>,
        scale: Vector3<f32>,
        start: Vector3<f32>,
        end: Vector3<f32>
    ) -> Option<WorldPath>
    {
        let direction = end - start;

        self.straight_line_free(entity, start, direction, scale, layer).then(||
        {
            WorldPath::new(vec![WorldPathNode::MoveTo(end), WorldPathNode::MoveTo(start)])
        })
    }

    pub fn pathfind(
        &self,
        entity: Entity,
        start: Vector3<f32>,
        end: Vector3<f32>
    ) -> Option<WorldPath>
    {
        let layer = self.pathfind_layer(entity);
        let scale = self.pathfind_scale(entity);

        if let Some(path) = self.pathfind_straight_inner(entity, layer, scale, start, end)
        {
            return Some(path);
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
        let mut limits = PathfindLimits(0);
        let mut debug_timer = DebugTimer::new();

        let tile_colliding = |pos| -> Option<f32>
        {
            self.world.tile(pos).and_then(|tile|
            {
                let tile_info = self.world.tile_info(*tile);

                (tile_info.colliding).then_some(self.world.tile_health(*tile))
            })
        };

        let target = TilePos::from(end);
        let start = TilePos::from(start);

        let mut steps = 0;

        let mut unexplored = BinaryHeap::from([
            Node{cost: 0.0, value: NodeValue{position: start, kind: NodeKind::MoveTo}}
        ]);

        let mut explored = HashMap::from([(start, NodeInfo{moves_from_start: 0.0, previous: None})]);

        while !unexplored.is_empty()
        {
            steps += 1;
            if steps > PATHFIND_MAX_STEPS || limits.over()
            {
                return None;
            }

            let current = unexplored.pop()?;

            if DebugConfig::is_enabled(DebugTool::DisplayPathfindAttempt)
            {
                debug_display_current(self.entities, current.clone());
            }

            if current.value.position == target
            {
                let current_z = Vector3::from(current.value.position.center_position()).z;
                let mut path: Vec<WorldPathNode> = vec![
                    WorldPathNode::MoveTo(Vector3::new(end.x, end.y, current_z)),
                    current.value.clone().into()
                ];

                current.path_to(&mut explored, &mut path, Into::into);

                debug_timer.print();
                return Some(crate::debug_time_this!{"simplify-path", self.simplify_path(entity, scale, layer, path)});
            }

            let below = current.value.position.offset(Pos3::new(0, 0, -1));
            let below_tile = self.world.tile(below).map(|tile| self.world.tile_info(*tile));

            let is_grounded = below_tile.map(|x| x.colliding).unwrap_or(false);

            let mut try_push = |
                explored: &mut HashMap<TilePos, NodeInfo>,
                position: TilePos,
                node: NodeKind,
                move_cost: f32
            |
            {
                let moves_from_start = explored[&current.value.position].moves_from_start;
                let new_cost = moves_from_start + move_cost;

                if let Some(explored) = explored.get_mut(&position)
                {
                    if explored.moves_from_start > new_cost
                    {
                        explored.moves_from_start = new_cost;
                        explored.previous = Some(current.clone());
                    }
                } else
                {
                    let info = NodeInfo{moves_from_start: new_cost, previous: Some(current.clone())};
                    explored.insert(position, info);

                    let goal_distance = Vector3::from(position.distance(target)).cast::<f32>().magnitude();

                    let cost = new_cost + goal_distance;

                    unexplored.push(Node{
                        cost,
                        value: NodeValue{position, kind: node}
                    });
                }
            };

            if let Some(SpecialTile::StairsDown) = below_tile.and_then(|x| x.special.as_ref())
            {
                try_push(
                    &mut explored,
                    current.value.position.offset(Pos3{x: 0, y: 0, z: -2}),
                    NodeKind::StairsMove{down: true},
                    1.0
                );
            }

            let current_tile_info = self.world.tile(current.value.position).map(|tile| self.world.tile_info(*tile));

            if let Some(SpecialTile::StairsUp) = current_tile_info.and_then(|x| x.special.as_ref())
            {
                try_push(
                    &mut explored,
                    current.value.position.offset(Pos3{x: 0, y: 0, z: 2}),
                    NodeKind::StairsMove{down: false},
                    1.0
                );
            }

            if is_grounded
            {
                PosDirection::iter_non_z().for_each(|direction|
                {
                    let position = current.value.position.offset(Pos3::from(direction));

                    let is_colliding_entity = |limits, debug_timer|
                    {
                        let layer = some_or_value!(layer, false);

                        self.is_colliding_entity(limits, entity, layer, scale, position, debug_timer)
                    };

                    if explored.contains_key(&position)
                        || (position == target)
                        || !is_colliding_entity(&mut limits, &mut debug_timer)
                    {
                        let base_move_cost = 1.0;
                        let (node, cost) = if let Some(health) = tile_colliding(position)
                        {
                            (NodeKind::BreakTile, base_move_cost + health / 0.0015)
                        } else
                        {
                            (NodeKind::MoveTo, base_move_cost)
                        };

                        try_push(&mut explored, position, node, cost);
                    }
                });
            } else
            {
                try_push(&mut explored, below, NodeKind::MoveTo, 1.0);
            }
        }

        debug_timer.print();
        None
    }

    fn is_colliding_entity(
        &self,
        limits: &mut PathfindLimits,
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

            let this = CollidingInfoRef::new(this_transform.clone(), this_collider);

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
        limits.add(inside_simulated);

        control.is_break()
    }

    fn simplify_path(
        &self,
        entity: Entity,
        scale: Vector3<f32>,
        layer: Option<ColliderLayer>,
        tiles: Vec<WorldPathNode>
    ) -> WorldPath
    {
        let mut simplified = Vec::new();

        let simplified_move = |simplified: &mut Vec<WorldPathNode>, start: usize, end: usize|
        {
            let mut check = 0;

            simplified.push(tiles[start].clone());

            let mut index = start + 1;
            while index < end
            {
                let is_next = (check + 1) == index;

                let is_tile_reachable = |tiles: &[WorldPathNode]|
                {
                    let start = tiles[check].as_move_to().unwrap();

                    let distance = tiles[index].as_move_to().unwrap() - start;

                    self.straight_line_free(entity, start, distance, scale, layer)
                };

                let is_reachable = is_next || is_tile_reachable(&tiles);

                if is_reachable
                {
                    index += 1;
                } else
                {
                    check = index - 1;

                    simplified.push(tiles[check].clone());
                }
            }
        };

        let mut start = 0;
        let mut end = 0;

        let limit = tiles.len();

        while start != limit
        {
            if end == limit
            {
                simplified_move(&mut simplified, start, limit);
                break;
            }

            if let WorldPathNode::MoveTo(_) = tiles[end]
            {
                end += 1;
            } else if start != end
            {
                simplified_move(&mut simplified, start, end);

                start = end;
            } else
            {
                simplified.push(tiles[end].clone());

                start += 1;
                end = start;
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

    fn pathfind_scale(&self, entity: Entity) -> Vector3<f32>
    {
        self.entities.collider(entity)
            .and_then(|x| x.override_transform.as_ref().map(|x| x.transform.scale))
            .or_else(|| self.entities.transform(entity).map(|x| x.scale))
            .unwrap_or_else(Vector3::zeros)
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
enum WorldPathNode
{
    MoveTo(Vector3<f32>),
    StairsMove{pos: TilePos, down: bool},
    BreakTile(TilePos)
}

impl From<NodeValue> for WorldPathNode
{
    fn from(value: NodeValue) -> Self
    {
        let position = value.position;

        match value.kind
        {
            NodeKind::MoveTo => Self::MoveTo(position.center_position().into()),
            NodeKind::StairsMove{down} => Self::StairsMove{pos: position, down},
            NodeKind::BreakTile => Self::BreakTile(position)
        }
    }
}

impl WorldPathNode
{
    fn as_move_to(&self) -> Option<Vector3<f32>>
    {
        if let Self::MoveTo(x) = self { Some(*x) } else { None }
    }

    // used for debug stuff only, dont rly care
    fn to_position(&self) -> Vector3<f32>
    {
        match self
        {
            Self::MoveTo(x) => *x,
            Self::StairsMove{pos, ..} => pos.center_position().into(),
            Self::BreakTile(x) => x.center_position().into()
        }
    }
}

pub enum WorldPathAction
{
    MoveDirection(Vector3<f32>),
    StairsMove{pos: TilePos, down: bool},
    Attack(TilePos)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldPath
{
    values: Vec<WorldPathNode>
}

impl WorldPath
{
    fn new(values: Vec<WorldPathNode>) -> Self
    {
        Self{values}
    }

    pub fn target(&self) -> Option<Vector3<f32>>
    {
        self.values.first().map(|x| x.as_move_to().expect("target must be a moveto"))
    }

    pub fn remove_current_target(&mut self)
    {
        self.values.pop();
    }

    pub fn action(
        &mut self,
        world: &World,
        near: f32,
        position: Vector3<f32>
    ) -> Option<WorldPathAction>
    {
        let target = self.values.last()?;

        match target
        {
            WorldPathNode::MoveTo(move_position) =>
            {
                let distance = move_position - position;
                if distance.magnitude() < near
                {
                    self.remove_current_target();

                    return self.action(world, near, position);
                }

                Some(WorldPathAction::MoveDirection(distance))
            },
            WorldPathNode::StairsMove{pos, down} =>
            {
                if (pos.center_position().z - position.z).abs() < TILE_SIZE
                {
                    self.remove_current_target();

                    self.action(world, near, position)
                } else
                {
                    Some(WorldPathAction::StairsMove{pos: *pos, down: *down})
                }
            },
            WorldPathNode::BreakTile(tile_pos) =>
            {
                if world.tile(*tile_pos).map(|tile| !tile.is_none()).unwrap_or(false)
                {
                    Some(WorldPathAction::Attack(*tile_pos))
                } else
                {
                    self.remove_current_target();

                    self.action(world, near, position)
                }
            }
        }
    }

    pub fn debug_display(&self, entities: &ClientEntities)
    {
        let amount = self.values.len();
        self.values.iter().cloned().enumerate().for_each(|(index, node)|
        {
            let position = node.to_position();

            let is_selected = (index + 1) == amount;

            let color = if is_selected
            {
                [1.0, 0.0, 0.0, 0.5]
            } else
            {
                [0.0, 0.0, 1.0, 0.5]
            };

            let name = match node
            {
                WorldPathNode::BreakTile(_) => "ui/close_button.png",
                WorldPathNode::StairsMove{down, ..} =>
                {
                    if down { "ui/down_icon.png" } else { "ui/up_icon.png" }
                },
                WorldPathNode::MoveTo(_) => "circle.png"
            };

            let entity = entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position,
                    scale: Vector3::repeat(TILE_SIZE * 0.3),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: name.into()
                    }.into()),
                    mix: Some(MixColor::color(color)),
                    above_world: true,
                    ..Default::default()
                }),
                ..Default::default()
            });

            entities.add_watcher(entity, Watcher::simple_one_frame());
        });

        self.values.iter().zip(self.values.iter().skip(1)).for_each(|(previous, current)|
        {
            let previous_position = previous.to_position();
            let current_position = current.to_position();

            if let Some(info) = line_info(previous_position, current_position, TILE_SIZE * 0.1, [0.2, 0.2, 1.0])
            {
                let entity = entities.push(true, info);
                entities.add_watcher(entity, Watcher::simple_one_frame());
            }
        });
    }
}

struct NodeInfo
{
    moves_from_start: f32,
    previous: Option<Node>
}

#[derive(Debug, Clone, Copy)]
enum NodeKind
{
    MoveTo,
    StairsMove{down: bool},
    BreakTile
}

#[derive(Debug, Clone)]
struct NodeValue
{
    position: TilePos,
    kind: NodeKind
}

#[derive(Debug, Clone)]
struct Node
{
    cost: f32,
    value: NodeValue
}

impl Node
{
    fn path_to<T, F: Fn(NodeValue) -> T>(
        self,
        explored: &mut HashMap<TilePos, NodeInfo>,
        path: &mut Vec<T>,
        f: F
    )
    {
        if let Some(node) = explored.remove(&self.value.position).unwrap().previous
        {
            path.push(f(node.value.clone()));
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
