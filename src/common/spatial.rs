use std::cmp::Ordering;

use nalgebra::Vector3;

use crate::common::Entity;


const MAX_DEPTH: usize = 9;

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

    fn random_sample<T: Clone>(values: &[T], amount: usize) -> Vec<T>
    {
        let s = fastrand::usize(0..values.len());

        values.iter().chain(values.iter()).skip(s).take(amount).cloned().collect()
    }

    pub fn new(mut infos: Vec<SpatialInfo>, depth: usize) -> Self
    {
        if depth > MAX_DEPTH || infos.len() < 2
        {
            return Self::new_leaf(infos);
        }

        let axis_i = depth % 3;

        let median = {
            const AMOUNT: usize = 15;

            if infos.len() < AMOUNT
            {
                infos.sort_unstable_by(|a, b|
                {
                    a.position.index(axis_i).partial_cmp(b.position.index(axis_i))
                        .unwrap_or(Ordering::Equal)
                });

                *infos[infos.len() / 2].position.index(axis_i)
            } else
            {
                let mut random_sample = Self::random_sample(&infos, AMOUNT);
                random_sample.sort_unstable_by(|a, b|
                {
                    a.position.index(axis_i).partial_cmp(b.position.index(axis_i))
                        .unwrap_or(Ordering::Equal)
                });

                *random_sample[AMOUNT / 2].position.index(axis_i)
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


#[cfg(test)]
mod tests
{
    use super::*;


    fn almost_equal(a: f32, b: f32)
    {
        assert!((a - b).abs() < 0.0001);
    }

    #[test]
    fn median()
    {
        let mut values = [5.3, 0.2, 11.3, 31.2].map(|x|
        {
            SpatialInfo{
                entity: Entity::from_raw(false, 0),
                position: Vector3::new(x, 0.0, 0.0),
                scale: Vector3::zeros()
            }
        });

        let x = KNode::find_median(&mut values, 0);

        almost_equal(x, 8.3);
    }
}
