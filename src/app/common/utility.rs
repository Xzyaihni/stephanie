use std::{
    f32,
    io::Write,
    fmt::Debug,
    fs::File,
    path::{Path, Component},
    ops::{Range, RangeInclusive}
};

use serde::{Deserialize, Serialize};

use nalgebra::Vector3;

pub use crate::{LOG_PATH, define_layers, some_or_value, some_or_return};


#[macro_export]
macro_rules! define_layers
{
    ($left:expr, $right:expr, $(($first:ident, $second:ident, $result:literal)),+) =>
    {
        #[allow(unreachable_patterns)]
        match ($left, $right)
        {
            $(
                (Self::$first, Self::$second) => $result,
                (Self::$second, Self::$first) => $result
            ),+
        }
    }
}

#[macro_export]
macro_rules! some_or_value
{
    ($value:expr, $return_value:expr) =>
    {
        match $value
        {
            Some(x) => x,
            None => return $return_value
        }
    }
}

#[macro_export]
macro_rules! some_or_return
{
    ($value:expr) =>
    {
        $crate::some_or_value!{$value, ()}
    }
}

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

pub fn pick_by_commonness<I, T, F>(
    this_commonness: f64,
    iter: I,
    f: F
) -> Option<T>
where
    I: Iterator<Item=T> + Clone,
    T: Copy,
    F: Fn(T) -> f64
{
    let scaled_commonness = |c: f64|
    {
        c.powf(this_commonness)
    };

    WeightedPicker::pick_from(fastrand::f64(), iter, move |value|
    {
        scaled_commonness(f(value))
    })
}

pub fn normalize_path(path: impl AsRef<Path>) -> String
{
    let mut components = Vec::new();
    path.as_ref().components().for_each(|component|
    {
        match component
        {
            Component::ParentDir =>
            {
                components.pop();
            },
            x =>
            {
                components.push(x);
            }
        }
    });

    components.into_iter().map(|x|
    {
        x.as_os_str().to_string_lossy().into_owned()
    }).reduce(|acc, x|
    {
        acc + "/" + &x
    }).unwrap_or_default()
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

pub trait EaseOut
{
    fn ease_out(&self, target: Self, decay: f32, dt: f32) -> Self;
}

impl EaseOut for f32
{
    fn ease_out(&self, target: Self, decay: f32, dt: f32) -> Self
    {
        ease_out(*self, target, decay, dt)
    }
}

impl EaseOut for [f32; 3]
{
    fn ease_out(&self, target: Self, decay: f32, dt: f32) -> Self
    {
        self.iter().zip(target)
            .map(|(current, target)| current.ease_out(target, decay, dt))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }
}

impl EaseOut for Vector3<f32>
{
    fn ease_out(&self, target: Self, decay: f32, dt: f32) -> Self
    {
        self.zip_map(&target, |a, b|
            {
                ease_out(a, b, decay, dt)
            })
    }
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

pub fn write_log(text: impl Into<String>)
{
    match File::options().append(true).create(true).open(LOG_PATH)
    {
        Ok(mut x) =>
        {
            x.write(text.into().as_bytes()).map(|_| {}).unwrap_or_else(|err|
            {
                eprintln!("error writing to log: {err}");
            });
        },
        Err(err) => eprintln!("error writing to log: {err}")
    }
}

pub fn insertion_sort_with<T, KeyGetter, Swapper, Sortable>(
    values: &mut [T],
    get_key: KeyGetter,
    mut swapper: Swapper
)
where
    Sortable: Ord,
    KeyGetter: Fn(&T) -> Sortable,
    Swapper: FnMut(&T, &T)
{
    let mut swap = |values: &mut [T], a, b|
    {
        swapper(&values[a], &values[b]);

        values.swap(a, b);
    };

    let mut current = 0;

    while current < values.len()
    {
        for i in (1..=current).rev()
        {
            let current = get_key(&values[i]);
            let other = get_key(&values[i - 1]);

            if current >= other
            {
                break;
            } else
            {
                swap(values, i, i - 1);
            }
        }

        current += 1;
    }
}

#[cfg(test)]
mod tests
{
    use std::iter;

    use super::*;


    #[test]
    fn insertion_sort()
    {
        for l in 1..10
        {
            let mut values: Vec<_> = iter::repeat_with(|| fastrand::i32(-10..10)).take(l)
                .collect();

            println!("sorting {values:?}");

            insertion_sort_with(&mut values, |x| *x, |a, b|
            {
                println!("swapping {a:?} and {b:?}");
            });

            println!("sorted {values:?}");

            values.iter().reduce(|acc, x|
            {
                if acc > x
                {
                    panic!("not sorted");
                }

                x
            });
        }
    }
}
