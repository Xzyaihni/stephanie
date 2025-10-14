use std::{
    f32,
    env,
    iter,
    borrow::{Cow, Borrow},
    cmp::Ordering,
    hash::Hash,
    io::Write,
    fmt::{Display, Debug},
    fs::File,
    collections::HashMap,
    path::{Path, Component},
    ops::{Index, Range, RangeInclusive}
};

use serde::{Deserialize, Serialize};

use nalgebra::{vector, ArrayStorage, Unit, Vector2, Vector3};

use yanyaengine::Transform;

pub use crate::{
    LOG_PATH,
    define_layers,
    define_layers_enum,
    some_or_value,
    some_or_false,
    some_or_return,
    common::{
        ENTITY_SCALE,
        watcher::*,
        render_info::*,
        EntityInfo,
        world::{TILE_SIZE, PosDirection}
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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct SortableF32(f32);

impl From<f32> for SortableF32
{
    fn from(value: f32) -> Self
    {
        if value.is_nan()
        {
            panic!("cant sort nans");
        }

        Self(value)
    }
}

impl Eq for SortableF32 {}

// this is okay because in the constructor im making sure it cant be a nan
// and afaik nans are the only reason floats dont have full ord
#[allow(clippy::derive_ord_xor_partial_ord)]
impl Ord for SortableF32
{
    fn cmp(&self, other: &Self) -> Ordering
    {
        self.0.partial_cmp(&other.0).unwrap()
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

    pub fn clear(&mut self)
    {
        self.normal.clear();
        self.back.clear();
    }

    pub fn contains_key(&self, k: &K) -> bool
    {
        self.normal.contains_key(k)
    }

    pub fn contains_value(&self, v: &V) -> bool
    {
        self.back.contains_key(v)
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

    pub fn pick_by(&self, get_weight: impl Fn(I::Item) -> f64) -> Option<I::Item>
    {
        self.pick_with(fastrand::f64(), get_weight)
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

pub fn opposite_angle(angle: f32) -> f32
{
    f32::consts::PI + angle
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

pub fn angle_to_direction_3d(angle: f32) -> Unit<Vector3<f32>>
{
    Unit::new_unchecked(vector![angle.cos(), -angle.sin(), 0.0])
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

pub fn project_onto_plane(normal: Unit<Vector3<f32>>, d: f32, p: Vector3<f32>) -> Vector3<f32>
{
    p - *normal * (p.dot(&normal) - d)
}

pub fn line_left_distance(p: Vector2<f32>, a: Vector2<f32>, b: Vector2<f32>) -> f32
{
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

pub fn line_on_left(p: Vector2<f32>, a: Vector2<f32>, b: Vector2<f32>) -> bool
{
    line_left_distance(p, a, b) > 0.0
}

pub fn line_parallel_side(p: Vector2<f32>, a: Vector2<f32>, b: Vector2<f32>) -> Ordering
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
    let check = match line_parallel_side(p, a, b)
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

pub fn cross_2d(
    Vector2{data: ArrayStorage([[ax, ay]]), ..}: Vector2<f32>,
    Vector2{data: ArrayStorage([[bx, by]]), ..}: Vector2<f32>
) -> f32
{
    ax * by - bx * ay
}

pub fn cross_3d(a: Vector3<f32>, b: Vector3<f32>) -> Vector3<f32>
{
    Vector3::new(
        cross_2d(a.yz(), b.yz()),
        cross_2d(a.zx(), b.zx()),
        cross_2d(a.xy(), b.xy())
    )
}

pub fn rotate_point(Vector2{data: ArrayStorage([[px, py]]), ..}: Vector2<f32>, angle: f32) -> Vector2<f32>
{
    let (asin, acos) = (-angle).sin_cos();

    vector![acos * px + asin * py, -asin * px + acos * py]
}

pub fn rotate_point_z_3d(p: Vector3<f32>, angle: f32) -> Vector3<f32>
{
    let r = rotate_point(p.xy(), angle);
    vector![r.x, r.y, p.z]
}

pub fn project_onto_2d(transform: &Transform, p: &Vector2<f32>) -> Vector2<f32>
{
    let scaled = transform.scale.xy().component_mul(p);
    rotate_point(scaled, transform.rotation) + transform.position.xy()
}

pub fn project_onto(transform: &Transform, p: &Vector3<f32>) -> Vector3<f32>
{
    let scaled = transform.scale.component_mul(p);
    rotate_point_z_3d(scaled, transform.rotation) + transform.position
}

pub fn with_z<T>(Vector2{data: ArrayStorage([[x, y]]), ..}: Vector2<T>, z: T) -> Vector3<T>
{
    vector![x, y, z]
}

#[derive(Debug, Clone, Copy)]
pub struct Line
{
    pub a: Vector2<f32>,
    pub b: Vector2<f32>
}

impl Line
{
    pub fn map(self, mut f: impl FnMut(Vector2<f32>) -> Vector2<f32>) -> Self
    {
        Self{
            a: f(self.a),
            b: f(self.b)
        }
    }
}

pub fn rectangle_edges(transform: &Transform) -> impl Iterator<Item=Line>
{
    let position = transform.position.xy();
    let scale = transform.scale;
    let rotation = transform.rotation;

    PosDirection::iter_non_z().map(move |x|
    {
        x.edge_line_2d(scale.xy()).map(|x| rotate_point(x, rotation) + position)
    })
}

fn intersection_lines_inner(line0: Line, line1: Line) -> (f32, f32, bool)
{
    let x1 = line0.a.x;
    let x2 = line0.b.x;
    let x3 = line1.a.x;
    let x4 = line1.b.x;

    let y1 = line0.a.y;
    let y2 = line0.b.y;
    let y3 = line1.a.y;
    let y4 = line1.b.y;

    let ll = x1 - x2;
    let lr = y3 - y4;
    let rl = y1 - y2;
    let rr = x3 - x4;

    let bottom = ll * lr - rl * rr;

    let x1x3 = x1 - x3;
    let y1y3 = y1 - y3;

    let t_top = x1x3 * lr - y1y3 * rr;
    let u_top = -(ll * y1y3 - rl * x1x3);

    fn intersecting_with(top: f32, bottom: f32) -> bool
    {
        let bottom_sign = bottom.signum();

        (0.0..=bottom.abs()).contains(&(top * bottom_sign))
    }

    let intersecting = {
        if (line0.a.x == line0.b.x) && (line1.a.x == line1.b.x) && (line0.a.x == line1.a.x)
        {
            intersecting_with(y1y3, rl - lr)
        } else if (line0.a.y == line0.b.y) && (line1.a.y == line1.b.y) && (line0.a.y == line1.a.y)
        {
            intersecting_with(x1x3, ll - rr)
        } else
        {
            intersecting_with(t_top, bottom) && intersecting_with(u_top, bottom)
        }
    };

    (t_top, bottom, intersecting)
}

pub fn is_intersection_lines(line0: Line, line1: Line) -> bool
{
    intersection_lines_inner(line0, line1).2
}

pub fn intersection_lines(line0: Line, line1: Line) -> Option<Vector2<f32>>
{
    let (t_top, bottom, intersecting) = intersection_lines_inner(line0, line1);

    if !intersecting
    {
        return None;
    }

    let t = t_top / bottom;

    let x1 = line0.a.x;
    let x2 = line0.b.x;

    let y1 = line0.a.y;
    let y2 = line0.b.y;

    Some(Vector2::new(x1 + t * (x2 - x1), y1 + t * (y2 - y1)))
}

pub fn aabb_bounds(transform: &Transform) -> Vector3<f32>
{
    let scale = transform.scale.xy();
    let rotation = transform.rotation;

    let a = rotate_point(vector![scale.x, -scale.y], rotation);
    let b = rotate_point(scale, rotation);

    vector![a.x.abs().max(b.x.abs()), a.y.abs().max(b.y.abs()), transform.scale.z]
}

pub fn aabb_points(transform: &Transform) -> (Vector2<f32>, Vector2<f32>)
{
    let half_size = aabb_bounds(transform).xy() * 0.5;

    let pos = transform.position.xy();

    (pos - half_size, pos + half_size)
}

pub fn rectangle_points(transform: &Transform) -> [Vector2<f32>; 4]
{
    let pos = transform.position.xy();
    let size = transform.scale.xy() * 0.5;
    let rotation = transform.rotation;

    [
        vector![-size.x, -size.y],
        vector![size.x, -size.y],
        vector![size.x, size.y],
        vector![-size.x, size.y]
    ].map(|x|
    {
        rotate_point(x, rotation) + pos
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

pub fn tile_marker_info(position: Vector3<f32>, color: [f32; 4], amount: usize, id: usize) -> EntityInfo
{
    debug_marker_info(
        position + with_z(Vector2::repeat(TILE_SIZE), 0.0),
        Vector2::repeat(TILE_SIZE),
        color,
        1.0,
        amount,
        id
    )
}

pub fn debug_marker_info(
    position: Vector3<f32>,
    scale: Vector2<f32>,
    color: [f32; 4],
    fill: f32,
    amount: usize,
    id: usize
) -> EntityInfo
{
    let start = position - with_z(scale * 0.5, 0.0);

    let per_row = (amount as f32).sqrt().ceil() as usize;

    let x = id % per_row;
    let y = id / per_row;

    let per_row = per_row as f32;
    let scale = scale / per_row;

    let position = (start + with_z(scale * 0.5, 0.0)) + with_z(Vector2::new(x as f32, y as f32).component_mul(&scale), 0.0);

    EntityInfo{
        transform: Some(Transform{
            position,
            scale: with_z(scale * fill, ENTITY_SCALE),
            ..Default::default()
        }),
        render: Some(RenderInfo{
            object: Some(RenderObjectKind::Texture{
                name: "solid.png".into()
            }.into()),
            above_world: true,
            mix: Some(MixColor::color(color)),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub fn line_info(
    start: Vector3<f32>,
    end: Vector3<f32>,
    thickness: f32,
    color: [f32; 3]
) -> Option<EntityInfo>
{
    let direction = end - start;
    let scale = Vector3::new(direction.xy().magnitude(), thickness, 1.0);

    direction_like_info("solid.png", start, with_z(direction.xy(), 0.0), scale, color)
}

pub fn direction_arrow_info(
    point: Vector3<f32>,
    direction: Vector3<f32>,
    arrow_scale: f32,
    color: [f32; 3]
) -> Option<EntityInfo>
{
    direction_like_info("arrow.png", point, direction, Vector3::repeat(arrow_scale), color)
}

fn direction_like_info(
    texture: impl Into<Cow<'static, str>>,
    point: Vector3<f32>,
    direction: Vector3<f32>,
    scale: Vector3<f32>,
    color: [f32; 3]
) -> Option<EntityInfo>
{
    Unit::try_new(direction.xy(), 0.001).map(|normal_direction|
    {
        let angle = normal_direction.y.atan2(normal_direction.x);
        let shift_amount = Vector3::new(normal_direction.x, normal_direction.y, 0.0) * (scale.x / 2.0);

        EntityInfo{
            transform: Some(Transform{
                position: point + shift_amount,
                scale,
                rotation: angle,
                ..Default::default()
            }),
            render: Some(RenderInfo{
                object: Some(RenderObjectKind::Texture{
                    name: texture.into()
                }.into()),
                mix: Some(MixColor{color: [color[0], color[1], color[2], 1.0], amount: 1.0, keep_transparency: true}),
                above_world: true,
                ..Default::default()
            }),
            ..Default::default()
        }
    })
}

pub fn from_upper_camel(s: &str) -> String
{
    let mut previous_uppercase = true;

    s.chars().flat_map(|c|
    {
        let is_uppercase = c.is_uppercase();
        let c = c.to_lowercase();

        if is_uppercase && !previous_uppercase
        {
            return iter::once(' ').chain(c).collect::<Vec<_>>();
        }

        previous_uppercase = is_uppercase;

        c.collect::<Vec<_>>()
    }).collect()
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

pub fn with_error<T, E: Display>(value: Result<T, E>) -> Option<T>
{
    match value
    {
        Ok(x) => Some(x),
        Err(err) =>
        {
            eprintln!("{err}");

            None
        }
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
