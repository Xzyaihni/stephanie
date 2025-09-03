use std::{
    cmp::Ordering,
    collections::{HashMap, BinaryHeap}
};

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    line_info,
    raycast,
    watcher::*,
    render_info::*,
    AnyEntities,
    EntityInfo,
    World,
    PosDirection,
    Pos3,
    world::{
        TILE_SIZE,
        ClientEntities,
        TilePos
    }
};


const PATHFIND_MAX_STEPS: usize = 1000;

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

            entities.push(true, EntityInfo{
                transform: Some(Transform{
                    position,
                    scale: Vector3::repeat(TILE_SIZE * 0.3),
                    ..Default::default()
                }),
                render: Some(RenderInfo{
                    object: Some(RenderObjectKind::Texture{
                        name: "circle.png".to_owned()
                    }.into()),
                    mix: Some(MixColor{keep_transparency: true, ..MixColor::color(color)}),
                    above_world: true,
                    ..Default::default()
                }),
                watchers: Some(Watchers::simple_one_frame()),
                ..Default::default()
            });
        });

        self.values.iter().zip(self.values.iter().skip(1)).for_each(|(previous, current)|
        {
            if let Some(info) = line_info(*previous, *current, TILE_SIZE * 0.1, [0.2, 0.2, 1.0])
            {
                entities.push(true, info);
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

pub fn pathfind(
    world: &World,
    scale: Vector3<f32>,
    start: Vector3<f32>,
    end: Vector3<f32>
) -> Option<WorldPath>
{
    let target = TilePos::from(end);
    let start = TilePos::from(start);

    if start.distance(target).z > 0
    {
        return None;
    }

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

        if current.value == target
        {
            let tiles = {
                let current_position: Vector3<f32> = current.value.center_position().into();
                let mut path = vec![Vector3::new(end.x, end.y, current_position.z), current_position];
                current.path_to(&mut explored, &mut path, |x| x.center_position().into());

                path
            };

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

                    raycast::swept_aabb_world(
                        world,
                        &Transform{
                            position: start,
                            scale,
                            ..Default::default()
                        },
                        distance
                    ).is_none()
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

            return Some(WorldPath::new(simplified));
        }

        let below = current.value.offset(Pos3::new(0, 0, -1));
        let is_grounded = !world.tile(below)?.is_none();

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

                if world.tile(position).map(|x| x.is_none()).unwrap_or(false)
                {
                    try_push(position);
                }
            });
        } else
        {
            try_push(below);
        }
    }

    None
}
