use std::{
    cmp::Ordering,
    ops::ControlFlow,
    cell::RefCell
};

use nalgebra::{Vector2, Vector3};

use crate::{
    debug_config::*,
    common::{
        some_or_value,
        some_or_return,
        unique_pairs_no_self,
        render_info::*,
        watcher::*,
        Transform,
        EntityInfo,
        AnyEntities,
        Entity,
        Collider,
        OverrideTransform,
        world::{
            CHUNK_SIZE,
            CLIENT_OVERMAP_SIZE_Z,
            TILE_SIZE,
            TilePos,
            overmap::{self, OvermapIndexing},
            chunk::{rounded_single, to_tile_single}
        },
        entity::{iterate_components_with, for_each_component, ClientEntities}
    }
};

#[allow(unused_imports)]
use crate::common::{ENTITY_SCALE, with_z, line_info};


const MAX_DEPTH: usize = 5;
const NODES_Z: usize = CHUNK_SIZE * CLIENT_OVERMAP_SIZE_Z;

fn node_z(mapper: &ZMapper, TilePos{chunk, local}: TilePos) -> Option<usize>
{
    Some(local.pos().z + mapper.to_local_z(chunk.0.z)? * CHUNK_SIZE)
}

fn node_z_value(mapper: &ZMapper, value: f32) -> Option<usize>
{
    Some(to_tile_single(value) + mapper.to_local_z(rounded_single(value))? * CHUNK_SIZE)
}

fn halfspace(median: f32, position: f32, half_scale: f32) -> Ordering
{
    let median_distance = position - median;

    if median_distance.abs() < half_scale
    {
        Ordering::Equal
    } else if median_distance < 0.0
    {
        Ordering::Less
    } else
    {
        Ordering::Greater
    }
}

#[derive(Debug, Clone)]
pub struct SpatialInfo
{
    pub entity: Entity,
    pub position: Vector3<f32>,
    pub half_scale: Vector3<f32>
}

#[derive(Debug)]
pub enum KNode
{
    Node{
        left: Box<KNode>,
        right: Box<KNode>,
        median: f32
    },
    Leaf{entities: Vec<Entity>}
}

impl KNode
{
    #[allow(dead_code)]
    pub fn empty() -> Self
    {
        Self::Leaf{entities: Vec::new()}
    }

    fn new_leaf(infos: Vec<SpatialInfo>) -> Self
    {
        Self::Leaf{entities: infos.into_iter().map(|x| x.entity).collect()}
    }

    fn random_sample<const AMOUNT: usize, T: Clone>(values: &[T]) -> Vec<T>
    {
        const OVERSAMPLE: usize = 2;

        let total = values.len();

        let difference = total - AMOUNT;
        if difference < AMOUNT / 2
        {
            return values.iter().skip(fastrand::usize(0..(difference + 1))).take(AMOUNT).cloned().collect();
        }

        let indices = (0..AMOUNT * OVERSAMPLE).try_fold(Vec::new(), |mut state, _|
        {
            let value = fastrand::usize(0..total);

            if !state.contains(&value)
            {
                state.push(value);

                if state.len() == AMOUNT
                {
                    return ControlFlow::Break(state);
                }
            }

            ControlFlow::Continue(state)
        });

        let indices = match indices
        {
            ControlFlow::Continue(x) => x,
            ControlFlow::Break(x) => x
        };

        indices.into_iter().map(|index| values[index].clone()).collect()
    }

    pub fn new(mut infos: Vec<SpatialInfo>, depth: usize) -> Self
    {
        if depth > MAX_DEPTH || infos.len() < 2
        {
            return Self::new_leaf(infos);
        }

        let axis_i = depth % 2;

        let median = {
            const AMOUNT: usize = 16;

            let axis_sort = |values: &mut [SpatialInfo]|
            {
                values.sort_unstable_by(|a, b|
                {
                    a.position.index(axis_i).partial_cmp(b.position.index(axis_i))
                        .unwrap_or(Ordering::Equal)
                });
            };

            let get_axis = |values: &[SpatialInfo], index: usize|
            {
                *values[index].position.index(axis_i)
            };

            if infos.len() < AMOUNT
            {
                axis_sort(&mut infos);

                (get_axis(&infos, infos.len() / 2 - 1) + get_axis(&infos, infos.len() / 2)) / 2.0
            } else
            {
                let mut random_sample = Self::random_sample::<AMOUNT, SpatialInfo>(&infos);
                axis_sort(&mut random_sample);

                (get_axis(&random_sample, AMOUNT / 2 - 1) + get_axis(&random_sample, AMOUNT / 2)) / 2.0
            }
        };

        let mut left_infos = Vec::new();
        let mut right_infos = Vec::new();

        infos.into_iter().for_each(|info|
        {
            match halfspace(median, info.position[axis_i], info.half_scale[axis_i])
            {
                Ordering::Equal =>
                {
                    // in both halfspaces

                    left_infos.push(info.clone());
                    right_infos.push(info);
                },
                Ordering::Less =>
                {
                    // in left halfspace

                    left_infos.push(info);
                },
                Ordering::Greater =>
                {
                    // in right halfspace

                    right_infos.push(info);
                }
            }
        });

        Self::Node{
            left: Box::new(Self::new(left_infos, depth + 1)),
            right: Box::new(Self::new(right_infos, depth + 1)),
            median
        }
    }

    fn try_possible_collisions_with_inner<Break>(
        &self,
        position: Vector2<f32>,
        half_scale: Vector2<f32>,
        depth: usize,
        f: &mut impl FnMut(Entity) -> ControlFlow<Break, ()>
    ) -> ControlFlow<Break, ()>
    {
        match self
        {
            Self::Node{left, right, median} =>
            {
                let axis_i = depth % 2;
                let new_depth = depth + 1;

                match halfspace(*median, position[axis_i], half_scale[axis_i])
                {
                    Ordering::Equal =>
                    {
                        left.try_possible_collisions_with_inner(position, half_scale, new_depth, f)?;
                        right.try_possible_collisions_with_inner(position, half_scale, new_depth, f)
                    },
                    Ordering::Less =>
                    {
                        left.try_possible_collisions_with_inner(position, half_scale, new_depth, f)
                    },
                    Ordering::Greater =>
                    {
                        right.try_possible_collisions_with_inner(position, half_scale, new_depth, f)
                    }
                }
            },
            Self::Leaf{entities} =>
            {
                entities.iter().copied().try_for_each(f)
            }
        }
    }

    pub fn try_possible_collisions_with<Break>(
        &self,
        position: Vector2<f32>,
        half_scale: Vector2<f32>,
        mut f: impl FnMut(Entity) -> ControlFlow<Break, ()>
    ) -> ControlFlow<Break, ()>
    {
        self.try_possible_collisions_with_inner(position, half_scale, 0, &mut f)
    }

    fn possible_pairs(&self, f: &mut impl FnMut(Entity, Entity))
    {
        match self
        {
            Self::Node{left, right, ..} =>
            {
                left.possible_pairs(f);
                right.possible_pairs(f);
            },
            Self::Leaf{entities} =>
            {
                unique_pairs_no_self(entities.iter().copied(), |a, b|
                {
                    f(a, b);
                });
            }
        }
    }

    pub fn try_fold<State, Break>(
        &self,
        s: State,
        f: &mut impl FnMut(State, Entity) -> ControlFlow<Break, State>
    ) -> ControlFlow<Break, State>
    {
        match self
        {
            Self::Node{left, right, ..} =>
            {
                let s = left.try_fold(s, f)?;

                right.try_fold(s, f)
            },
            Self::Leaf{entities} =>
            {
                entities.iter().copied().try_fold(s, f)
            }
        }
    }

    #[cfg(not(debug_assertions))]
    fn debug_display(
        &self,
        _client_entities: &ClientEntities,
        _path: Vec<bool>
    )
    {
        unreachable!()
    }

    #[cfg(debug_assertions)]
    fn debug_display(
        &self,
        client_entities: &ClientEntities,
        path: Vec<(f32, bool)>
    )
    {
        match self
        {
            Self::Node{left, right, median} =>
            {
                let mut left_path = path.clone();
                left_path.push((*median, true));

                left.debug_display(client_entities, left_path);

                let mut right_path = path;
                right_path.push((*median, false));

                right.debug_display(client_entities, right_path);
            },
            Self::Leaf{entities} =>
            {
                entities.iter().for_each(|entity|
                {
                    if let Some(transform) = client_entities.transform(*entity)
                    {
                        let z = transform.position.z;
                        let thickness = ENTITY_SCALE * 0.02;
                        let (_, position, scale) = path.iter().enumerate().fold(
                            ([(f32::NEG_INFINITY, f32::INFINITY), (f32::NEG_INFINITY, f32::INFINITY)], transform.position.xy(), transform.scale.xy()),
                            |(mut line, position, mut scale), (index, (median, state))|
                            {
                                let axis_i = index % 2;

                                let mut opposite_axis = Vector2::zeros();
                                opposite_axis[1 - axis_i] = 1.0;

                                {
                                    if !*state
                                    {
                                        line[axis_i].0 = *median;
                                    } else
                                    {
                                        line[axis_i].1 = *median;
                                    }

                                    let mut start = Vector2::new(line[0].0, line[1].0);
                                    start[axis_i] = *median;

                                    let mut end = Vector2::new(line[0].1, line[1].1);
                                    end[axis_i] = *median;

                                    let other_axis = 1 - axis_i;

                                    let start_inf = start[other_axis] == f32::NEG_INFINITY;
                                    let end_inf = end[other_axis] == f32::INFINITY;

                                    if start_inf && end_inf
                                    {
                                        start[other_axis] = transform.position[other_axis] - 100.0;
                                        end[other_axis] = transform.position[other_axis] + 100.0;
                                    } else
                                    {
                                        if start_inf
                                        {
                                            start[other_axis] = end[other_axis] - 100.0;
                                        }

                                        if end_inf
                                        {
                                            end[other_axis] = start[other_axis] + 100.0;
                                        }
                                    }

                                    if let Some(line) = line_info(with_z(start, z), with_z(end, z), ENTITY_SCALE * 0.05, [0.4, 0.0, 0.0])
                                    {
                                        client_entities.push(true, line);
                                    }
                                }

                                let mut axis = Vector2::zeros();
                                axis[axis_i] = 1.0;

                                let start = position - opposite_axis.component_mul(&(scale * 0.5));
                                let end = position + opposite_axis.component_mul(&(scale * 0.5));

                                if let Some(line) = line_info(with_z(start, z), with_z(end, z), thickness, [0.0, 0.0, 0.5])
                                {
                                    client_entities.push(true, line);
                                }

                                scale[axis_i] *= 0.5;

                                let shift = axis.component_mul(&(scale * 0.5));
                                (line, position + if *state { -shift } else { shift }, scale)
                            });

                        client_entities.push(true, EntityInfo{
                            transform: Some(Transform{
                                position: with_z(position, transform.position.z),
                                scale: scale.xyx(),
                                ..Default::default()
                            }),
                            render: Some(RenderInfo{
                                object: Some(RenderObjectKind::Texture{
                                    name: "solid.png".into()
                                }.into()),
                                mix: Some(MixColor{keep_transparency: true, ..MixColor::color([1.0, 0.0, 0.0, 0.3])}),
                                above_world: true,
                                z_level: ZLevel::BelowFeet,
                                ..Default::default()
                            }),
                            watchers: Some(Watchers::simple_one_frame()),
                            ..Default::default()
                        });
                    }
                });
            }
        }
    }

    fn debug_print(&self, depth: usize) -> (usize, bool, Box<dyn FnOnce()>)
    {
        match self
        {
            Self::Node{left, right, ..} =>
            {
                let new_depth = depth + 1;

                let (left_len, is_left_last, left_f) = left.debug_print(new_depth);
                let (right_len, is_right_last, right_f) = right.debug_print(new_depth);

                let total = left_len + right_len;

                let f = Box::new(move ||
                {
                    if !is_left_last
                    {
                        eprintln!("{1:0$}left with {left_len} values {{", new_depth, ' ');
                    }

                    left_f();

                    if !is_left_last
                    {
                        eprintln!("{1:0$}}}", new_depth, ' ');
                    }

                    if !is_right_last
                    {
                        eprintln!("{1:0$}right with {right_len} values {{", new_depth, ' ');
                    }

                    right_f();

                    if !is_right_last
                    {
                        eprintln!("{1:0$}}}", new_depth, ' ');
                    }
                });

                (total, false, f)
            },
            Self::Leaf{entities} =>
            {
                let len = entities.len();
                (len, true, Box::new(move || eprintln!("{1:0$}leaf with {len} values", depth + 1, ' ')))
            }
        }
    }
}

#[derive(Debug)]
struct ZMapper
{
    position: i32,
    size: usize
}

impl ZMapper
{
    pub fn to_local_z(&self, z: i32) -> Option<usize>
    {
        overmap::to_local_z(self.position, self.size, z)
    }
}

#[derive(Debug)]
pub struct SpatialGrid
{
    z_mapper: ZMapper,
    follow_position: Vector3<f32>,
    pub z_nodes: [KNode; NODES_Z]
}

impl SpatialGrid
{
    pub fn new(
        entities: &ClientEntities,
        mapper: &impl OvermapIndexing,
        follow_target: Entity
    ) -> Self
    {
        fn player_z(entities: &ClientEntities, mapper: &impl OvermapIndexing) -> Option<usize>
        {
            iterate_components_with!(entities, player, find_map, |entity, _|
            {
                entities.transform(entity).and_then(|x|
                {
                    let pos = TilePos::from(x.position);
                    let player_chunk = mapper.to_local(mapper.player_position())?.pos.z;

                    Some(player_chunk * CHUNK_SIZE + pos.local.pos().z)
                })
            })
        }

        let z_mapper = ZMapper{position: mapper.player_position().0.z, size: mapper.size().z};

        let mut queued = [const { Vec::new() }; NODES_Z];
        for_each_component!(entities, collider, |entity, collider: &RefCell<Collider>|
        {
            let collider = collider.borrow();

            if collider.sleeping
            {
                return;
            }

            let (half_scale, position) = {
                let transform = some_or_return!(entities.transform(entity));

                let (transform, position) = if let Some(OverrideTransform{
                    transform: override_transform,
                    override_position
                }) = collider.override_transform.as_ref()
                {
                    let position = if *override_position
                    {
                        override_transform.position
                    } else
                    {
                        override_transform.position + transform.position
                    };

                    (override_transform, position)
                } else
                {
                    (&*transform, transform.position)
                };

                (collider.half_bounds(transform), position)
            };

            let z = {
                if let Some(x) = node_z_value(&z_mapper, position.z)
                {
                    x
                } else
                {
                    return eprintln!("position {position} is out of range");
                }
            };

            let info = SpatialInfo{
                entity,
                half_scale,
                position
            };

            queued[z].push(info);

            if DebugConfig::is_enabled(DebugTool::DisplaySpatial)
            {
                if let Some(player_z) = player_z(entities, mapper)
                {
                    if player_z == z
                    {
                        entities.push(true, EntityInfo{
                            transform: Some(Transform{
                                position,
                                scale: half_scale * 2.0,
                                ..Default::default()
                            }),
                            render: Some(RenderInfo{
                                object: Some(RenderObjectKind::Texture{
                                    name: "solid.png".into()
                                }.into()),
                                mix: Some(MixColor{keep_transparency: true, ..MixColor::color([1.0, 1.0, 0.0, 0.3])}),
                                above_world: true,
                                z_level: ZLevel::BelowFeet,
                                ..Default::default()
                            }),
                            watchers: Some(Watchers::simple_one_frame()),
                            ..Default::default()
                        });
                    }
                }
            }
        });

        let z_nodes = queued.map(|queued| KNode::new(queued, 0));

        z_nodes.iter().enumerate().for_each(|(z, node)|
        {
            if DebugConfig::is_enabled(DebugTool::DisplaySpatial)
            {
                if let Some(player_z) = player_z(entities, mapper)
                {
                    if player_z == z
                    {
                        node.debug_display(entities, vec![]);
                    }
                }
            }

            if DebugConfig::is_enabled(DebugTool::Spatial)
            {
                let (amount, _, f) = node.debug_print(0);
                eprintln!("spatial {z} has {amount} entities");

                if DebugConfig::is_enabled(DebugTool::SpatialFull)
                {
                    f();
                }
            }
        });

        let follow_position = entities.transform(follow_target).map(|x| x.position).unwrap_or_else(Vector3::zeros);

        Self{
            z_mapper,
            follow_position,
            z_nodes
        }
    }

    pub fn z_of(&self, value: f32) -> Option<usize>
    {
        node_z_value(&self.z_mapper, value)
    }

    pub fn possible_pairs(&self, mut f: impl FnMut(Entity, Entity))
    {
        self.z_nodes.iter().for_each(|node|
        {
            node.possible_pairs(&mut f);
        });
    }

    pub fn try_fold<State, Break>(
        &self,
        s: State,
        mut f: impl FnMut(State, Entity) -> ControlFlow<Break, State>
    ) -> ControlFlow<Break, State>
    {
        self.z_nodes.iter().try_fold(s, |s, node| node.try_fold(s, &mut f))
    }

    pub fn try_for_each<Break>(
        &self,
        mut f: impl FnMut(Entity) -> ControlFlow<Break, ()>
    ) -> ControlFlow<Break, ()>
    {
        self.try_fold((), move |_, x| f(x))
    }

    pub fn try_for_each_near<Break>(
        &self,
        pos: TilePos,
        f: impl FnMut(Entity) -> ControlFlow<Break, ()>
    ) -> ControlFlow<Break, ()>
    {
        let z = some_or_value!(node_z(&self.z_mapper, pos), ControlFlow::Continue(()));

        self.z_nodes[z].try_possible_collisions_with(
            Vector3::from(pos.center_position()).xy(),
            Vector2::repeat(TILE_SIZE * 0.5),
            f
        )
    }

    pub fn inside_simulated(&self, position: Vector3<f32>, scale: f32) -> bool
    {
        ((position.z - self.follow_position.z).abs() < TILE_SIZE * 2.5)
            && ((position.xy().metric_distance(&self.follow_position.xy()) - scale) < TILE_SIZE * 30.0)
    }
}
