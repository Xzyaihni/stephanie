use std::{
    cmp::Ordering,
    ops::ControlFlow,
    cell::RefCell
};

use nalgebra::Vector3;

use crate::{
    debug_config::*,
    common::{
        unique_pairs_no_self,
        Entity,
        Collider,
        world::{CHUNK_SIZE, CLIENT_OVERMAP_SIZE_Z, Pos3, overmap::OvermapIndexing},
        entity::{for_each_component, ClientEntities}
    }
};


const MAX_DEPTH: usize = 6;
const NODES_Z: usize = CHUNK_SIZE * CLIENT_OVERMAP_SIZE_Z;

#[derive(Debug, Clone)]
pub struct SpatialInfo
{
    pub entity: Entity,
    pub position: Vector3<f32>,
    pub scale: Vector3<f32>
}

#[derive(Debug)]
enum KNode
{
    Node{left: Box<KNode>, right: Box<KNode>},
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
            let this_scale = *info.scale.index(axis_i);
            let this_distance = *info.position.index(axis_i);

            let median_distance = this_distance - median;

            if median_distance.abs() < this_scale
            {
                // in both halfspaces

                left_infos.push(info.clone());
                right_infos.push(info);
            } else if median_distance < 0.0
            {
                // in left halfspace
                left_infos.push(info);
            } else
            {
                // in right halfspace
                right_infos.push(info);
            }
        });

        Self::Node{
            left: Box::new(Self::new(left_infos, depth + 1)),
            right: Box::new(Self::new(right_infos, depth + 1))
        }
    }

    fn possible_pairs(&self, f: &mut impl FnMut(Entity, Entity))
    {
        match self
        {
            Self::Node{left, right} =>
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

    fn debug_print(&self, depth: usize) -> (usize, bool, Box<dyn FnOnce()>)
    {
        match self
        {
            Self::Node{left, right} =>
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
pub struct SpatialGrid
{
    z_nodes: [KNode; NODES_Z]
}

impl SpatialGrid
{
    pub fn new(
        entities: &ClientEntities,
        mapper: &impl OvermapIndexing
    ) -> Self
    {
        let mut queued = [const { Vec::new() }; NODES_Z];
        for_each_component!(entities, collider, |entity, collider: &RefCell<Collider>|
        {
            let collider = collider.borrow();

            let mut transform = entities.transform(entity).unwrap().clone();
            if let Some(scale) = collider.scale
            {
                transform.scale = scale;
            }

            let position = transform.position;

            let z = {
                let position = Pos3::from(position);

                let chunk_z = position.rounded().0.z;

                let chunk_z_local = if let Some(x) = mapper.to_local_z(chunk_z)
                {
                    x
                } else
                {
                    return eprintln!("position {position} is out of range");
                };

                position.to_tile().z + chunk_z_local * CHUNK_SIZE
            };

            let info = SpatialInfo{
                entity,
                scale: collider.bounds(&transform),
                position
            };

            queued[z].push(info);
        });

        let z_nodes = queued.map(|queued| KNode::new(queued, 0));

        z_nodes.iter().enumerate().for_each(|(z, node)|
        {
            if DebugConfig::is_enabled(DebugTool::Spatial)
            {
                let (amount, _, f) = node.debug_print(0);

                eprintln!("spatial {z} has {amount} entities");
                f();
            }
        });

        Self{
            z_nodes
        }
    }

    pub fn possible_pairs(&mut self, mut f: impl FnMut(Entity, Entity))
    {
        self.z_nodes.iter().for_each(|node|
        {
            node.possible_pairs(&mut f);
        });
    }
}
