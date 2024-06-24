use std::{
    f32,
    ops::{Range, RangeInclusive}
};

use serde::{Deserialize, Serialize};

use nalgebra::Vector3;


pub struct WeightedPicker<I>
{
    total: f64,
    values: I
}

impl<I> WeightedPicker<I>
where
    I: IntoIterator + Clone,
    I::Item: Copy
{
    pub fn new(total: f64, values: I) -> Self
    {
        Self{total, values}
    }

    pub fn pick_from(
        random_value: f64,
        values: I,
        get_weight: impl Fn(I::Item) -> f64
    ) -> Option<I::Item>
    {
        let total = values.clone().into_iter().map(&get_weight).sum();

        Self::new(total, values).pick_with(random_value, get_weight)
    }

    pub fn pick_with(
        &self,
        random_value: f64,
        get_weight: impl Fn(I::Item) -> f64
    ) -> Option<I::Item>
    {
        let mut random_value = random_value * self.total;

        self.values.clone().into_iter().find(|value|
        {
            let weight = get_weight(*value);
            random_value -= weight;

            random_value <= 0.0
        })
    }
}

pub fn random_f32(range: RangeInclusive<f32>) -> f32
{
    fastrand::f32() * (range.end() - range.start()) + range.start()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeededRandom(u64);

impl From<u64> for SeededRandom
{
    fn from(value: u64) -> Self
    {
        Self(value)
    }
}

impl SeededRandom
{
    pub fn new() -> Self
    {
        Self(fastrand::u64(0..u64::MAX))
    }

    pub fn set_state(&mut self, value: u64)
    {
        self.0 = value;
    }

    // splitmix64 by sebastiano vigna
    pub fn next_u64(&mut self) -> u64
    {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);

        let x = self.0;

        let x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        let x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);

        x ^ (x >> 31)
    }

    pub fn next_u64_between(&mut self, range: Range<u64>) -> u64
    {
        let difference = range.end - range.start;

        range.start + self.next_u64() % difference
    }

    pub fn next_usize_between(&mut self, range: Range<usize>) -> usize
    {
        let difference = range.end - range.start;

        range.start + (self.next_u64() as usize) % difference
    }

    pub fn next_f32(&mut self) -> f32
    {
        let x = self.next_u64();

        x as f32 / u64::MAX as f32
    }

    pub fn next_f64(&mut self) -> f64
    {
        let x = self.next_u64();

        x as f64 / u64::MAX as f64
    }

    pub fn next_f32_between(&mut self, range: RangeInclusive<f32>) -> f32
    {
        let x = self.next_f32();

        let size = range.end() - range.start();

        range.start() + x * size
    }

    pub fn next_bool(&mut self) -> bool
    {
        self.next_u64() % 2 == 0
    }
}

pub fn random_rotation() -> f32
{
    fastrand::f32() * (f32::consts::PI * 2.0)
}

pub fn short_rotation(rotation: f32) -> f32
{
    let rotation = rotation % (f32::consts::PI * 2.0);

    if rotation > f32::consts::PI
    {
        rotation - 2.0 * f32::consts::PI
    } else if rotation < -f32::consts::PI
    {
        rotation + 2.0 * f32::consts::PI
    } else
    {
        rotation
    }
}

pub fn angle_between(a: Vector3<f32>, b: Vector3<f32>) -> f32
{
    let offset = b - a;

    let angle_between = offset.y.atan2(-offset.x);

    short_rotation(angle_between)
}

// thanks freya holmer
pub fn ease_out(current: f32, target: f32, decay: f32, dt: f32) -> f32
{
    target + (current - target) * (-decay * dt).exp()
}

pub fn lerp_dt(current: f32, target: f32, amount: f32, dt: f32) -> f32
{
    target + (current - target) * (1.0 - amount).powf(dt)
}

pub fn lerp(x: f32, y: f32, a: f32) -> f32
{
    (1.0 - a) * x + y * a
}

pub fn get_two_mut<T>(s: &mut [T], one: usize, two: usize) -> (&mut T, &mut T)
{
    if one > two
    {
        let (left, right) = s.split_at_mut(one);

        (&mut right[0], &mut left[two])
    } else
    {
        let (left, right) = s.split_at_mut(two);

        (&mut left[one], &mut right[0])
    }
}

pub fn group_by<T>(predicate: impl Fn(&T, &T) -> bool, values: impl Iterator<Item=T>) -> Vec<Vec<T>>
where
    // i dont NEED this but i dont like rearranging functions
    T: Copy
{
    let mut groups = Vec::new();

    values.for_each(|value|
    {
        groups.iter_mut().find_map(|group: &mut Vec<T>|
        {
            let head = group.first().expect("all groups have at least 1 element");

            predicate(&value, head).then(||
            {
                group.push(value);
            })
        }).unwrap_or_else(||
        {
            groups.push(vec![value]);
        });
    });

    groups
}
