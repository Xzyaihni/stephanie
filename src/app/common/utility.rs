use std::{
    f32,
    env,
    borrow::Borrow,
    cmp::Ordering,
    hash::Hash,
    io::Write,
    fmt::Debug,
    fs::File,
    collections::HashMap,
    path::{Path, Component},
    ops::{Index, Range, RangeInclusive}
};

use serde::{Deserialize, Serialize};

use nalgebra::{Unit, Vector2, Vector3};

use yanyaengine::Transform;

pub use crate::{
    LOG_PATH,
    define_layers,
    define_layers_enum,
    some_or_value,
    some_or_false,
    some_or_return,
    common::{
        watcher::*,
        render_info::*,
        EntityInfo
    }
};


#[macro_export]
macro_rules! define_layers_enum
{
    (
        $left:expr,
        $right:expr,
        $base:ident,
        $(($first:ident, $second:ident, $result:expr)),+
        $((order_dependent, $first_order:ident, $second_order:ident, $result_order:expr)),*) =>
    {
        #[allow(unreachable_patterns)]
        match ($left, $right)
        {
            $(
                ($base::$first, $base::$second) => $result,
                ($base::$second, $base::$first) => $result,
            )+
            $(
                ($base::$first_order, $base::$second_order) => $result_order
            ),*
        }
    }
}

#[macro_export]
macro_rules! define_layers
{
    ($left:expr, $right:expr, $(($first:ident, $second:ident, $result:expr)),+) =>
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
        {
            match $value
            {
                Some(x) => x,
                None => return $return_value
            }
        }
    }
}

#[macro_export]
macro_rules! some_or_false
{
    ($value:expr) =>
    {
        $crate::some_or_value!{$value, false}
    }
}

#[macro_export]
macro_rules! some_or_return
{
    ($value:expr) =>
    {
        $crate::some_or_value!{$value, Default::default()}
    }
}

#[derive(Debug, Clone)]
pub struct BiMap<K, V>
{
    normal: HashMap<K, V>,
    back: HashMap<V, K>
}

impl<K: Hash + Eq + Clone, V: Hash + Eq + Clone> FromIterator<(K, V)> for BiMap<K, V>
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item=(K, V)>
    {
        let normal: HashMap<K, V> = iter.into_iter().collect();
        let back = normal.iter().map(|(k, v)| (v.clone(), k.clone())).collect();

        Self{
            normal,
            back
        }
    }
}

impl<Q: Eq + Hash + Debug + ?Sized, K: Hash + Eq + Borrow<Q>, V: Hash + Eq> Index<&Q> for BiMap<K, V>
{
    type Output = V;

    fn index(&self, index: &Q) -> &Self::Output
    {
        self.get(index).unwrap_or_else(||
        {
            panic!("`{index:?}` doesnt exist")
        })
    }
}

impl<K: Hash + Eq, V: Hash + Eq> BiMap<K, V>
{
    pub fn new() -> Self
    {
        Self{normal: HashMap::new(), back: HashMap::new()}
    }

    pub fn contains_key(&self, k: &K) -> bool
    {
        self.normal.contains_key(k)
    }

    pub fn insert(&mut self, key: K, value: V)
    where
        K: Clone,
        V: Clone
    {
        self.normal.insert(key.clone(), value.clone());
        self.back.insert(value, key);
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        Q: Eq + Hash + ?Sized,
        K: Borrow<Q>
    {
        self.normal.get(key)
    }

    pub fn get_back(&self, key: &V) -> Option<&K>
    {
        self.back.get(key)
    }

    pub fn iter(&self) -> impl Iterator<Item=(&K, &V)>
    {
        self.normal.iter()
    }

    pub fn iter_front(&self) -> impl Iterator<Item=&K>
    {
        self.normal.keys()
    }

    pub fn iter_back(&self) -> impl Iterator<Item=&V>
    {
        self.back.keys()
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

pub fn f32_to_range(range: RangeInclusive<f32>, value: f32) -> f32
{
    value * (range.end() - range.start()) + range.start()
}

pub fn random_f32(range: RangeInclusive<f32>) -> f32
{
    f32_to_range(range, fastrand::f32())
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

    pub fn choice<T, I, V>(&mut self, values: V) -> T
    where
        I: ExactSizeIterator<Item=T>,
        V: IntoIterator<Item=T, IntoIter=I>
    {
        let mut values = values.into_iter();
        let id = self.next_usize_between(0..values.len());

        values.nth(id).unwrap()
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

    let angle_between = (-offset.y).atan2(offset.x);

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

impl<const N: usize> EaseOut for [f32; N]
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

impl EaseOut for Vector2<f32>
{
    fn ease_out(&self, target: Self, decay: f32, dt: f32) -> Self
    {
        self.zip_map(&target, |a, b|
            {
                ease_out(a, b, decay, dt)
            })
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

pub fn project_onto_plane(normal: Unit<Vector3<f32>>, d: f32, p: Vector3<f32>) -> Vector3<f32>
{
    p - *normal * (p.dot(&normal) - d)
}

pub fn point_line_side(p: Vector2<f32>, a: Vector2<f32>, b: Vector2<f32>) -> Ordering
{
    let x = project_onto_line(p, a, b);
    if x < 0.0
    {
        Ordering::Less
    } else if x > 1.0
    {
        Ordering::Greater
    } else
    {
        Ordering::Equal
    }
}

pub fn project_onto_line(p: Vector2<f32>, a: Vector2<f32>, b: Vector2<f32>) -> f32
{
    let ad = b.metric_distance(&p);
    let cd = a.metric_distance(&b);
    let bd = a.metric_distance(&p);

    let cosa = (ad.powi(2) - bd.powi(2) - cd.powi(2)) / (-2.0 * bd * cd);

    cosa * bd / cd
}

pub fn point_line_distance(p: Vector2<f32>, a: Vector2<f32>, b: Vector2<f32>) -> f32
{
    let check = match point_line_side(p, a, b)
    {
        Ordering::Equal =>
        {
            let diff = b - a;

            return cross_2d(diff, a - p).abs() / diff.magnitude();
        },
        Ordering::Less => a,
        Ordering::Greater => b
    };

    p.metric_distance(&check)
}

pub fn cross_2d(a: Vector2<f32>, b: Vector2<f32>) -> f32
{
    a.x * b.y - b.x * a.y
}

pub fn cross_3d(a: Vector3<f32>, b: Vector3<f32>) -> Vector3<f32>
{
    Vector3::new(
        cross_2d(a.yz(), b.yz()),
        cross_2d(a.zx(), b.zx()),
        cross_2d(a.xy(), b.xy())
    )
}

pub fn rotate_point(p: Vector2<f32>, angle: f32) -> Vector2<f32>
{
    let (asin, acos) = (-angle).sin_cos();

    Vector2::new(acos * p.x + asin * p.y, -asin * p.x + acos * p.y)
}

pub fn rotate_point_z_3d(p: Vector3<f32>, angle: f32) -> Vector3<f32>
{
    let r = rotate_point(p.xy(), angle);
    Vector3::new(r.x, r.y, p.z)
}

pub fn project_onto(transform: &Transform, p: &Vector3<f32>) -> Vector3<f32>
{
    let scaled = transform.scale.component_mul(p);
    rotate_point_z_3d(scaled, transform.rotation) + transform.position
}

pub fn rectangle_points(transform: &Transform) -> [Vector2<f32>; 4]
{
    let size = transform.scale;
    let pos = transform.position;
    let rotation = transform.rotation;

    let x_shift = Vector2::new(size.x / 2.0, 0.0);
    let y_shift = Vector2::new(0.0, size.y / 2.0);

    let pos = pos.xy();

    let left_middle = pos - x_shift;
    let right_middle = pos + x_shift;

    [
        left_middle - y_shift,
        right_middle - y_shift,
        right_middle + y_shift,
        left_middle + y_shift
    ].map(|x|
    {
        rotate_point(x - pos, rotation) + pos
    })
}

// calls the function for each unique combination (excluding (self, self) pairs)
pub fn unique_pairs_no_self<I, T>(mut iter: I, mut f: impl FnMut(T, T))
where
    T: Clone,
    I: Iterator<Item=T> + Clone
{
    iter.clone().for_each(|a|
    {
        iter.by_ref().next();
        iter.clone().for_each(|b| f(a.clone(), b));
    });
}

pub fn direction_arrow_info(
    point: Vector3<f32>,
    direction: Vector3<f32>,
    arrow_scale: f32,
    color: [f32; 3]
) -> Option<EntityInfo>
{
    Unit::try_new(direction.xy(), 0.01).map(|normal_direction|
    {
        let angle = normal_direction.y.atan2(normal_direction.x);
        let shift_amount = Vector3::new(normal_direction.x, normal_direction.y, 0.0)
            * (arrow_scale / 2.0);

        EntityInfo{
            transform: Some(Transform{
                position: point + shift_amount,
                scale: Vector3::repeat(arrow_scale),
                rotation: angle,
                ..Default::default()
            }),
            render: Some(RenderInfo{
                object: Some(RenderObjectKind::Texture{
                    name: "arrow.png".to_owned()
                }.into()),
                z_level: ZLevel::Door,
                mix: Some(MixColor{color: [color[0], color[1], color[2], 1.0], amount: 1.0, keep_transparency: true}),
                aspect: Aspect::KeepMax,
                ..Default::default()
            }),
            watchers: Some(Watchers::simple_one_frame()),
            ..Default::default()
        }
    })
}

pub fn debug_env() -> Option<String>
{
    env::var("STEPHANIE_DEBUG").ok()
}

pub fn is_debug_env(s: impl AsRef<str>) -> bool
{
    let ds = some_or_value!(debug_env(), false);
    let ds: &str = ds.as_ref();
    ds == s.as_ref()
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

pub fn write_log_ln(text: impl Into<String>)
{
    let mut text = text.into();
    text.push('\n');

    write_log(text)
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
