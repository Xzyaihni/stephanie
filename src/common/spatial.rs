use std::{cmp::Ordering, ops::ControlFlow};

use nalgebra::Vector3;

use crate::common::Entity;


const MAX_DEPTH: usize = 10;

pub type CellPos = Vector3<i32>;

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

        let axis_i = depth % 3;

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

    fn possible_pairs(&self, f: &mut impl FnMut(&[Entity]))
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
                f(entities);
            }
        }
    }
}

#[derive(Debug)]
pub struct SpatialGrid
{
    node: KNode
}

impl SpatialGrid
{
    pub fn new() -> Self
    {
        Self{
            node: KNode::empty()
        }
    }

    pub fn build(&mut self, infos: impl Iterator<Item=SpatialInfo>)
    {
        let queued: Vec<SpatialInfo> = infos.collect();

        self.node = KNode::new(queued, 0);
    }

    pub fn possible_pairs(&self, mut f: impl FnMut(&[Entity]))
    {
        self.node.possible_pairs(&mut f);
    }
}
