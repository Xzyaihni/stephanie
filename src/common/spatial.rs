use std::{
    cmp::Ordering,
    ops::ControlFlow,
    cell::{RefCell, Ref}
};

use nalgebra::{Vector2, Vector3};

use crate::{
    debug_config::*,
    common::{
        some_or_value,
        some_or_return,
        unique_pairs_no_self,
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
use crate::common::{
    render_info::*,
    watcher::*,
    ENTITY_SCALE,
    with_z,
    line_info,
    Transform,
    EntityInfo,
    AnyEntities
};


pub const NODES_Z: usize = CHUNK_SIZE * CLIENT_OVERMAP_SIZE_Z;

const MAX_DEPTH: usize = 5;
const RANDOM_SAMPLE_AMOUNT: usize = 16;

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

fn axis_sort(values: &mut [SpatialInfo], axis_i: usize)
{
    values.sort_unstable_by(|a, b|
    {
        a.position.index(axis_i).partial_cmp(b.position.index(axis_i))
            .unwrap_or(Ordering::Equal)
    });
}

fn get_axis(values: &[SpatialInfo], axis_i: usize, index: usize) -> f32
{
    *values[index].position.index(axis_i)
}

#[derive(Debug, Clone, Copy)]
pub struct SpatialInfo
{
    pub entity: Entity,
    pub position: Vector2<f32>,
    pub half_scale: Vector2<f32>
}

#[derive(Debug)]
pub enum KNode
{
    Node{
        left: Box<KNode>,
        right: Box<KNode>,
        median: f32
    },
    Leaf{entities: Vec<SpatialInfo>}
}

impl KNode
{
    #[allow(dead_code)]
    pub fn empty() -> Self
    {
        Self::Leaf{entities: Vec::new()}
    }

    fn new_leaf(entities: Vec<SpatialInfo>) -> Self
    {
        Self::Leaf{entities}
    }

    fn with_random_sample(values: &mut [SpatialInfo], axis_i: usize) -> f32
    {
        fn with_samples(random_sample: &mut [SpatialInfo], axis_i: usize) -> f32
        {
            axis_sort(random_sample, axis_i);

            (get_axis(random_sample, axis_i, RANDOM_SAMPLE_AMOUNT / 2 - 1) + get_axis(random_sample, axis_i, RANDOM_SAMPLE_AMOUNT / 2)) / 2.0
        }

        let total = values.len();

        let difference = total - RANDOM_SAMPLE_AMOUNT;

        let start = fastrand::usize(0..(difference + 1));
        with_samples(&mut values[start..(start + RANDOM_SAMPLE_AMOUNT)], axis_i)
    }

    pub fn new(mut infos: Vec<SpatialInfo>, depth: usize) -> Self
    {
        if depth > MAX_DEPTH || infos.len() < 2
        {
            return Self::new_leaf(infos);
        }

        let axis_i = depth % 2;

        let median = {
            if infos.len() < RANDOM_SAMPLE_AMOUNT
            {
                axis_sort(&mut infos, axis_i);

                (get_axis(&infos, axis_i, infos.len() / 2 - 1) + get_axis(&infos, axis_i, infos.len() / 2)) / 2.0
            } else
            {
                Self::with_random_sample(&mut infos, axis_i)
            }
        };

        let half_len = infos.len() / 2;

        let mut left_infos = Vec::with_capacity(half_len);
        let mut right_infos = Vec::with_capacity(half_len);

        infos.into_iter().for_each(|info|
        {
            match halfspace(median, info.position[axis_i], info.half_scale[axis_i])
            {
                Ordering::Equal =>
                {
                    // in both halfspaces

                    left_infos.push(info);
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
        f: &mut impl FnMut(SpatialInfo) -> ControlFlow<Break, ()>
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
        mut f: impl FnMut(SpatialInfo) -> ControlFlow<Break, ()>
    ) -> ControlFlow<Break, ()>
    {
        self.try_possible_collisions_with_inner(position, half_scale, 0, &mut f)
    }

    fn possible_pairs(&self, f: &mut impl FnMut(SpatialInfo, SpatialInfo))
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
        f: &mut impl FnMut(State, SpatialInfo) -> ControlFlow<Break, State>
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
                entities.iter().for_each(|info|
                {
                    let entity = info.entity;
                    if let Some(transform) = client_entities.transform(entity)
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
                                        let entity = client_entities.push(true, line);
                                        client_entities.add_watcher(entity, Watcher::simple_one_frame());
                                    }
                                }

                                let mut axis = Vector2::zeros();
                                axis[axis_i] = 1.0;

                                let start = position - opposite_axis.component_mul(&(scale * 0.5));
                                let end = position + opposite_axis.component_mul(&(scale * 0.5));

                                if let Some(line) = line_info(with_z(start, z), with_z(end, z), thickness, [0.0, 0.0, 0.5])
                                {
                                    let entity = client_entities.push(true, line);
                                    client_entities.add_watcher(entity, Watcher::simple_one_frame());
                                }

                                scale[axis_i] *= 0.5;

                                let shift = axis.component_mul(&(scale * 0.5));
                                (line, position + if *state { -shift } else { shift }, scale)
                            });

                        let entity = client_entities.push(true, EntityInfo{
                            transform: Some(Transform{
                                position: with_z(position, transform.position.z),
                                scale: scale.xyx(),
                                ..Default::default()
                            }),
                            render: Some(RenderInfo{
                                object: Some(RenderObjectKind::Texture{
                                    name: "solid.png".into()
                                }.into()),
                                mix: Some(MixColor::color([1.0, 0.0, 0.0, 0.3])),
                                above_world: true,
                                z_level: ZLevel::BelowFeet,
                                ..Default::default()
                            }),
                            ..Default::default()
                        });

                        client_entities.add_watcher(entity, Watcher::simple_one_frame());
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

fn player_z(entities: &ClientEntities, mapper: &impl OvermapIndexing) -> Option<usize>
{
    iterate_components_with!(entities, player, find_map, |entity, _|
    {
        entities.transform(entity).and_then(|x|
        {
            let pos = TilePos::from(x.position);
            let player_chunk = mapper.to_local_z(mapper.player_position().0.z)?;

            Some(player_chunk * CHUNK_SIZE + pos.local.pos().z)
        })
    })
}

fn info_of(
    entities: &ClientEntities,
    entity: Entity,
    collider: Ref<Collider>,
    z_mapper: &ZMapper
) -> Option<(SpatialInfo, usize)>
{
    let (half_scale, position) = {
        let transform = entities.transform(entity)?;

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
            eprintln!("position {position} is out of range");
            return None;
        }
    };

    let info = SpatialInfo{
        entity,
        half_scale: half_scale.xy(),
        position: position.xy()
    };

    Some((info, z))
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
        mapper: impl OvermapIndexing,
        follow_target: Entity
    ) -> Self
    {
        let z_mapper = ZMapper{position: mapper.player_position().0.z, size: mapper.size().z};

        let mut queued = [const { Vec::new() }; NODES_Z];
        for_each_component!(entities, collider, |entity, collider: &RefCell<Collider>|
        {
            let collider = collider.borrow();

            if collider.sleeping
            {
                return;
            }

            let (info, z) = some_or_return!(info_of(entities, entity, collider, &z_mapper));

            queued[z].push(info);
        });

        let this = Self{
            z_mapper,
            follow_position: entities.transform(follow_target).map(|x| x.position).unwrap_or_else(Vector3::zeros),
            z_nodes: queued.map(|queued| KNode::new(queued, 0))
        };

        this.debug_z_nodes(entities, &mapper);

        this
    }

    pub fn rebuild_spatial(
        &mut self,
        entities: &ClientEntities,
        mapper: impl OvermapIndexing,
        follow_target: Entity
    )
    {
        self.z_mapper.position = mapper.player_position().0.z;
        self.follow_position = entities.transform(follow_target).map(|x| x.position).unwrap_or_else(Vector3::zeros);

        let mut queued = [const { Vec::new() }; NODES_Z];
        for_each_component!(entities, collider, |entity, collider: &RefCell<Collider>|
        {
            let collider = collider.borrow();

            if collider.sleeping
            {
                return;
            }

            let (info, z) = some_or_return!(info_of(entities, entity, collider, &self.z_mapper));

            queued[z].push(info);
        });

        self.z_nodes = queued.map(|queued| KNode::new(queued, 0));

        self.debug_z_nodes(entities, &mapper);
    }

    fn debug_z_nodes(&self, entities: &ClientEntities, mapper: &impl OvermapIndexing)
    {
        if DebugConfig::is_disabled(DebugTool::DisplaySpatial) && DebugConfig::is_disabled(DebugTool::Spatial)
        {
            return;
        }

        self.z_nodes.iter().enumerate().for_each(|(z, node)|
        {
            if player_z(entities, mapper) != Some(z)
            {
                return;
            }

            if DebugConfig::is_enabled(DebugTool::DisplaySpatial)
            {
                node.debug_display(entities, vec![]);
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
    }

    pub fn z_of(&self, value: f32) -> Option<usize>
    {
        node_z_value(&self.z_mapper, value)
    }

    pub fn possible_pairs(&self, mut f: impl FnMut(SpatialInfo, SpatialInfo))
    {
        self.z_nodes.iter().for_each(|node|
        {
            node.possible_pairs(&mut f);
        });
    }

    pub fn try_fold<State, Break>(
        &self,
        s: State,
        mut f: impl FnMut(State, SpatialInfo) -> ControlFlow<Break, State>
    ) -> ControlFlow<Break, State>
    {
        self.z_nodes.iter().try_fold(s, |s, node| node.try_fold(s, &mut f))
    }

    pub fn try_for_each<Break>(
        &self,
        mut f: impl FnMut(SpatialInfo) -> ControlFlow<Break, ()>
    ) -> ControlFlow<Break, ()>
    {
        self.try_fold((), move |_, x| f(x))
    }

    pub fn try_for_each_near<Break>(
        &self,
        pos: TilePos,
        f: impl FnMut(SpatialInfo) -> ControlFlow<Break, ()>
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
