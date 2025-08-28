use std::{
    f32,
    iter,
    fmt::{self, Debug, Display},
    ops::{Index, IndexMut, Deref, DerefMut}
};

use serde::{Serialize, Deserialize};

use crate::{
    debug_config::*,
    common::{
        Damage,
        DamageType,
        Side1d,
        Side2d,
        Damageable
    }
};

pub use human::*;

mod human;


pub const WINDED_OXYGEN: f32 = 0.2;

type DebugName = <DebugConfig as DebugConfigTrait>::DebugName;

macro_rules! simple_getter
{
    ($name:ident) =>
    {
        simple_getter!($name, f32);
    };
    ($name:ident, $t:ty) =>
    {
        pub fn $name(&self) -> $t
        {
            match self
            {
                Self::Human(x) => x.$name()
            }
        }
    }
}

pub trait FieldGet
{
    type T<'a, O>
    where
        Self: 'a,
        O: 'a;
}

pub struct RefHumanPartFieldGet;
impl FieldGet for RefHumanPartFieldGet
{
    type T<'a, O> = &'a HumanPart<O>
    where
        Self: 'a,
        O: 'a;
}

pub struct RefOrganFieldGet;
impl FieldGet for RefOrganFieldGet
{
    type T<'a, O> = &'a O
    where
        Self: 'a,
        O: 'a;
}

pub struct RefMutHumanPartFieldGet;
impl FieldGet for RefMutHumanPartFieldGet
{
    type T<'a, O> = &'a mut HumanPart<O>
    where
        Self: 'a,
        O: 'a;
}

pub struct RefMutOrganFieldGet;
impl FieldGet for RefMutOrganFieldGet
{
    type T<'a, O> = &'a mut O
    where
        Self: 'a,
        O: 'a;
}

pub trait PartFieldGetter<F: FieldGet>
{
    type V<'a>
    where
        F: 'a;

    fn get<'a, O: Organ + 'a>(value: F::T<'a, O>) -> Self::V<'a>;
}

impl PartFieldGetter<RefHumanPartFieldGet> for ()
{
    type V<'a> = ();

    fn get<'a, O: Organ + 'a>(_value: &'a HumanPart<O>) -> Self::V<'a> { }
}

impl PartFieldGetter<RefOrganFieldGet> for ()
{
    type V<'a> = ();

    fn get<'a, O: Organ + 'a>(_value: &'a O) -> Self::V<'a> { }
}

macro_rules! simple_field_getter
{
    ($name:ident, $t:ty, $f:ident) =>
    {
        pub struct $name;

        impl PartFieldGetter<RefHumanPartFieldGet> for $name
        {
            type V<'a> = &'a $t;

            fn get<'a, O: Organ + 'a>(value: &'a HumanPart<O>) -> Self::V<'a> { &value.$f }
        }

        impl PartFieldGetter<RefMutHumanPartFieldGet> for $name
        {
            type V<'a> = &'a mut $t;

            fn get<'a, O: Organ + 'a>(value: &'a mut HumanPart<O>) -> Self::V<'a> { &mut value.$f }
        }
    }
}

simple_field_getter!{BoneHealthGetter, Health, bone}
simple_field_getter!{MuscleHealthGetter, Health, muscle}
simple_field_getter!{SkinHealthGetter, Health, skin}
simple_field_getter!{SizeGetter, f64, size}

impl PartFieldGetter<RefOrganFieldGet> for SizeGetter
{
    type V<'a> = &'a f64;

    fn get<'a, O: Organ + 'a>(value: &'a O) -> Self::V<'a> { value.size() }
}

pub struct AverageHealthGetter;
impl PartFieldGetter<RefHumanPartFieldGet> for AverageHealthGetter
{
    type V<'a> = f32;

    fn get<'a, O: Organ + 'a>(value: &'a HumanPart<O>) -> Self::V<'a> { value.average_health() }
}

impl PartFieldGetter<RefOrganFieldGet> for AverageHealthGetter
{
    type V<'a> = Option<f32>;

    fn get<'a, O: Organ + 'a>(value: &'a O) -> Self::V<'a> { value.average_health() }
}

struct DamagerGetter;
impl PartFieldGetter<RefMutHumanPartFieldGet> for DamagerGetter
{
    type V<'a> = Box<dyn FnOnce(Damage) -> Option<Damage> + 'a>;

    fn get<'a, O: Organ + 'a>(value: &'a mut HumanPart<O>) -> Self::V<'a>
    {
        Box::new(|damage|
        {
            value.damage(damage)
        })
    }
}

impl PartFieldGetter<RefMutOrganFieldGet> for DamagerGetter
{
    type V<'a> = Box<dyn FnOnce(Damage) -> Option<Damage> + 'a>;

    fn get<'a, O: Organ + 'a>(value: &'a mut O) -> Self::V<'a>
    {
        Box::new(|damage|
        {
            let data = value.damage(damage.direction.side, damage.data);

            data.map(|data| Damage{data, ..damage})
        })
    }
}

struct AccessedGetter;
impl PartFieldGetter<RefMutHumanPartFieldGet> for AccessedGetter
{
    type V<'a> = Box<dyn FnOnce(&mut dyn FnMut(ChangedKind)) + 'a>;

    fn get<'a, O: Organ + 'a>(value: &'a mut HumanPart<O>) -> Self::V<'a>
    {
        Box::new(|f|
        {
            value.consume_accessed(f)
        })
    }
}

impl PartFieldGetter<RefMutOrganFieldGet> for AccessedGetter
{
    type V<'a> = bool;

    fn get<'a, O: Organ + 'a>(value: &'a mut O) -> Self::V<'a>
    {
        value.consume_accessed()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Anatomy
{
    Human(HumanAnatomy)
}

impl Anatomy
{
    simple_getter!(speed);
    simple_getter!(strength);
    simple_getter!(oxygen_speed);
    simple_getter!(oxygen, SimpleHealth);
    simple_getter!(vision);
    simple_getter!(vision_angle);
    simple_getter!(is_crawling, bool);
    simple_getter!(is_dead, bool);

    pub fn get_human<F>(
        &self,
        id: AnatomyId
    ) -> Option<Option<<F as PartFieldGetter<RefHumanPartFieldGet>>::V<'_>>>
    where
        F: PartFieldGetter<RefHumanPartFieldGet>,
        F: for<'a> PartFieldGetter<RefOrganFieldGet, V<'a>=<F as PartFieldGetter<RefHumanPartFieldGet>>::V<'a>>
    {
        let human = self.as_human()?;
        Some(human.body().get::<F>(id))
    }

    pub fn as_human(&self) -> Option<&HumanAnatomy>
    {
        #[allow(irrefutable_let_patterns)]
        if let Self::Human(x) = self
        {
            Some(x)
        } else
        {
            None
        }
    }

    pub fn override_crawling(&mut self, state: bool)
    {
        match self
        {
            Self::Human(x) => x.override_crawling(state)
        }
    }

    pub fn oxygen_mut(&mut self) -> &mut SimpleHealth
    {
        match self
        {
            Self::Human(x) => x.oxygen_mut()
        }
    }

    pub fn external_oxygen_change_mut(&mut self) -> &mut f32
    {
        match self
        {
            Self::Human(x) => x.external_oxygen_change_mut()
        }
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        match self
        {
            Self::Human(x) => x.set_speed(speed)
        }
    }

    pub fn update(&mut self, dt: f32) -> bool
    {
        match self
        {
            Self::Human(x) => x.update(dt)
        }
    }

    pub fn take_killed(&mut self) -> bool
    {
        match self
        {
            Self::Human(x) => x.take_killed()
        }
    }

    pub fn for_accessed_parts(&mut self, f: impl FnMut(ChangedPart))
    {
        match self
        {
            Self::Human(x) => x.for_accessed_parts(f)
        }
    }
}

impl Damageable for Anatomy
{
    fn damage(&mut self, damage: Damage) -> Option<Damage>
    {
        match self
        {
            Self::Human(x) => x.damage(damage)
        }
    }

    fn is_full(&self) -> bool
    {
        match self
        {
            Self::Human(x) => x.is_full()
        }
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        match self
        {
            Self::Human(x) => x.heal(amount)
        }
    }
}

pub fn health_iter_mut_helper(side: Side2d, x: &mut impl HealthIterate) -> Vec<&mut HealthField>
{
    x.health_sided_iter_mut(side).collect()
}

pub type HealthField = ChangeTracking<Health>;

pub trait HealthIterate
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>;
    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>;
}

pub trait HealReceiver: HealthIterate
{
    fn is_full(&self) -> bool
    {
        self.health_iter().all(|x| (**x).is_full())
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        let mut pool = amount;

        loop
        {
            let mut current = self.health_iter().filter(|x| !(***x).is_full()).count();

            if current == 0
            {
                break;
            }

            for health in self.health_sided_iter_mut(Side2d::default()).filter(|x| !(***x).is_full())
            {
                let heal_amount = pool / current as f32;
                pool -= heal_amount;

                pool += health.heal_remainder(heal_amount).unwrap_or(0.0);

                if pool <= 0.0
                {
                    return None;
                }

                current -= 1;
            }
        }

        Some(pool)
    }
}

pub trait DamageReceiver: HealReceiver
{
    fn damage(
        &mut self,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        if let DamageType::AreaEach(_) = &damage
        {
            self.health_sided_iter_mut(side).for_each(|x|
            {
                x.damage_pierce(damage, 1.0);
            });

            None
        } else
        {
            self.damage_normal(side, damage)
        }
    }

    fn damage_normal(
        &mut self,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.health_sided_iter_mut(side).try_fold(damage, |acc, x|
        {
            x.damage_pierce(acc, 1.0)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangedKind
{
    Bone,
    Muscle,
    Skin
}

impl Display for ChangedKind
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Bone => write!(f, "bone"),
            Self::Muscle => write!(f, "muscle"),
            Self::Skin => write!(f, "skin")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangedPart
{
    Part(HumanPartId, Option<ChangedKind>),
    Organ(OrganId)
}

impl Display for ChangedPart
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Part(HumanPartId::Torso, Some(ChangedKind::Muscle)) => write!(f, "thoracic diaphragm"),
            Self::Part(HumanPartId::Pelvis, Some(ChangedKind::Muscle)) => write!(f, "gluteal muscles"),
            Self::Part(id, kind) =>
            {
                match kind
                {
                    Some(ChangedKind::Bone) => write!(f, "{}", id.bone_to_string()),
                    Some(kind) => write!(f, "{id} {kind}"),
                    None => Display::fmt(id, f)
                }
            },
            Self::Organ(id) => Display::fmt(id, f)
        }
    }
}

impl ChangedPart
{
    pub fn whole(id: AnatomyId) -> Self
    {
        match id
        {
            AnatomyId::Part(x) => Self::Part(x, None),
            AnatomyId::Organ(x) => Self::Organ(x)
        }
    }

    pub fn iter() -> impl Iterator<Item=Self>
    {
        HumanPartId::iter().map(|x| Self::Part(x, Some(ChangedKind::Bone)))
            .chain(HumanPartId::iter().map(|x| Self::Part(x, Some(ChangedKind::Muscle))))
            .chain(HumanPartId::iter().map(|x| Self::Part(x, Some(ChangedKind::Skin))))
            .chain(OrganId::iter().map(Self::Organ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SimpleHealth
{
    pub max: f32,
    pub current: f32
}

impl Display for SimpleHealth
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{:.3}/{:.3}", self.current, self.max)
    }
}

impl From<f32> for SimpleHealth
{
    fn from(value: f32) -> Self
    {
        Self::new(value)
    }
}

impl SimpleHealth
{
    pub fn new(max: f32) -> Self
    {
        Self{max, current: max}
    }

    pub fn set_max(&mut self, new_max: f32)
    {
        self.max = new_max;

        if self.current > self.max
        {
            self.current = self.max;
        }
    }

    pub fn change(&mut self, change: f32)
    {
        self.current = (self.current + change).clamp(0.0, self.max);
    }

    pub fn subtract_hp(&mut self, amount: f32)
    {
        self.change(-amount);
    }

    pub fn heal_remainder(&mut self, amount: f32) -> Option<f32>
    {
        let remain = self.max - self.current;
        if remain < amount
        {
            self.current = self.max;

            Some(amount - remain)
        } else
        {
            self.current += amount;

            None
        }
    }

    pub fn current(&self) -> f32
    {
        self.current
    }

    pub fn fraction(&self) -> Option<f32>
    {
        if self.max == 0.0 { return None; }

        Some(self.current / self.max)
    }

    pub fn is_zero(&self) -> bool
    {
        self.current == 0.0
    }

    pub fn is_full(&self) -> bool
    {
        self.current == self.max
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Health
{
    pub block: f32,
    pub health: SimpleHealth
}

impl Debug for Health
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "Health {{ ({:.3}) {} }}", self.block, self.health)
    }
}

impl Health
{
    pub fn new(block: f32, max: f32) -> Self
    {
        debug_assert!((0.0..=1.0).contains(&block));

        Self{block, health: SimpleHealth::new(max)}
    }

    pub fn zero() -> Self
    {
        Self::new(0.0, 0.0)
    }

    pub fn fraction(&self) -> Option<f32>
    {
        self.health.fraction()
    }

    pub fn is_zero(&self) -> bool
    {
        self.health.is_zero()
    }

    pub fn is_full(&self) -> bool
    {
        self.health.is_full()
    }

    pub fn current(&self) -> f32
    {
        self.health.current()
    }

    pub fn heal_remainder(&mut self, amount: f32) -> Option<f32>
    {
        self.health.heal_remainder(amount)
    }

    pub fn damage_pierce(&mut self, damage: DamageType, sharpness_scale: f32) -> Option<DamageType>
    {
        match damage
        {
            DamageType::AreaEach(x) =>
            {
                self.simple_pierce(x);
                None
            },
            DamageType::Blunt(damage) =>
            {
                self.simple_pierce(damage).map(DamageType::Blunt)
            },
            DamageType::Sharp{sharpness, damage} =>
            {
                self.pierce_with(sharpness * sharpness_scale, damage).map(|damage|
                {
                    DamageType::Sharp{sharpness, damage}
                })
            },
            DamageType::Bullet(damage) =>
            {
                self.simple_pierce(damage).map(DamageType::Bullet)
            }
        }
    }

    fn simple_pierce(&mut self, damage: f32) -> Option<f32>
    {
        self.pierce_with(0.0, damage)
    }

    fn pierce_with(&mut self, sharpness: f32, damage: f32) -> Option<f32>
    {
        let pass = if self.is_zero()
        {
            damage
        } else
        {
            damage * (1.0 - (self.block * (1.0 - sharpness))).clamp(0.0, 1.0)
        };

        self.health.subtract_hp(damage);

        if pass <= 0.0
        {
            None
        } else
        {
            Some(pass)
        }
    }
}

#[derive(Clone)]
pub struct BodyPartInfo
{
    pub muscle_toughness: f32,
    pub skin_toughness: f32
}

impl From<HumanAnatomyInfo> for BodyPartInfo
{
    fn from(info: HumanAnatomyInfo) -> Self
    {
        Self{
            muscle_toughness: info.muscle_toughness,
            skin_toughness: info.skin_toughness
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChangeTracking<T>
{
    accessed: bool,
    value: T
}

impl<T: Debug> Debug for ChangeTracking<T>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        self.value.fmt(f)
    }
}

impl<T> Deref for ChangeTracking<T>
{
    type Target = T;

    fn deref(&self) -> &Self::Target
    {
        &self.value
    }
}

impl<T> DerefMut for ChangeTracking<T>
{
    fn deref_mut(&mut self) -> &mut Self::Target
    {
        self.accessed = true;
        &mut self.value
    }
}

impl<T> From<T> for ChangeTracking<T>
{
    fn from(value: T) -> Self
    {
        Self{accessed: false, value}
    }
}

impl<T> ChangeTracking<T>
{
    fn consume_accessed(&mut self) -> bool
    {
        let accessed = self.accessed;

        self.accessed = false;

        accessed
    }
}

pub trait Organ: DamageReceiver + Debug
{
    fn size(&self) -> &f64;

    fn is_broken(&self) -> bool { self.average_health().unwrap_or(0.0) == 0.0 }

    fn consume_accessed(&mut self) -> bool { unimplemented!() }

    fn average_health(&self) -> Option<f32>
    {
        let (total, sum) = self.health_iter().filter_map(|x| x.fraction()).fold((0, 0.0), |(total, sum), x|
        {
            (total + 1, sum + x)
        });

        (total != 0).then_some(sum / total as f32)
    }
}

impl HealthIterate for ()
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        [].into_iter()
    }

    fn health_sided_iter_mut(&mut self, _side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        [].into_iter()
    }
}

impl HealReceiver for () {}
impl DamageReceiver for () {}

impl Organ for ()
{
    fn average_health(&self) -> Option<f32> { None }
    fn size(&self) -> &f64 { &0.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyPart<Contents=()>
{
    name: DebugName,
    pub bone: ChangeTracking<Health>,
    pub skin: ChangeTracking<Health>,
    pub muscle: ChangeTracking<Health>,
    size: f64,
    contents: Contents
}

impl<Contents: HealthIterate> HealthIterate for BodyPart<Contents>
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        iter::once(&self.skin)
            .chain(iter::once(&self.muscle))
            .chain(iter::once(&self.bone))
            .chain(self.contents.health_iter())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        iter::once(&mut self.skin)
            .chain(iter::once(&mut self.muscle))
            .chain(iter::once(&mut self.bone))
            .chain(self.contents.health_sided_iter_mut(side))
    }
}

impl<Contents: Organ> HealReceiver for BodyPart<Contents> {}

impl<Contents> BodyPart<Contents>
{
    pub fn new(
        name: DebugName,
        info: BodyPartInfo,
        bone: f32,
        size: f64,
        contents: Contents
    ) -> Self
    {
        Self::new_full(
            name,
            Health::new(0.99, bone),
            Health::new(0.5, info.skin_toughness),
            Health::new(0.9, info.muscle_toughness * 5.0),
            size,
            contents
        )
    }

    pub fn new_full(
        name: DebugName,
        bone: Health,
        skin: Health,
        muscle: Health,
        size: f64,
        contents: Contents
    ) -> Self
    {
        Self{
            name,
            bone: bone.into(),
            skin: skin.into(),
            muscle: muscle.into(),
            size,
            contents
        }
    }

    pub fn contents(&self) -> &Contents
    {
        &self.contents
    }

    pub fn is_broken(&self) -> bool
    {
        self.bone.current() == 0.0
            && self.skin.current() == 0.0
            && self.muscle.current() == 0.0
    }

    fn speed_multiply(&self, base: f32, override_muscle: Option<f32>) -> f32
    {
        let muscle_health = override_muscle.unwrap_or_else(||
        {
            self.muscle.fraction().unwrap_or(0.0)
        });

        let health_mult = (self.bone.fraction().unwrap_or(0.0) * 0.9 + 0.1) * muscle_health;

        base * health_mult
    }

    fn average_health(&self) -> f32
    {
        let (total, sum) = iter::once(self.bone.fraction().unwrap_or(0.0))
            .chain(self.muscle.fraction())
            .chain(self.skin.fraction())
            .fold((0, 0.0), |(total, sum), x| (total + 1, sum + x));

        sum / total as f32
    }
}

impl<Contents: Organ> BodyPart<Contents>
{
    fn damage(&mut self, damage: Damage) -> Option<Damage>
    {
        if DebugConfig::is_enabled(DebugTool::PrintDamage)
        {
            eprintln!("damaging {} for {damage:?}", self.name.name());
        }

        let direction = damage.direction;

        self.damage_inner(direction.side, damage.data).map(|damage|
        {
            Damage{direction, data: damage}
        })
    }

    fn damage_inner(
        &mut self,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.contents.damage(
            side,
            self.bone.damage_pierce(
                self.muscle.damage_pierce(
                    self.skin.damage_pierce(damage, 0.0)?,
                    1.0)?,
                0.0)?)
    }
}

impl<Contents: Organ> BodyPart<Contents>
{
    fn consume_accessed(&mut self, mut f: impl FnMut(ChangedKind))
    {
        if self.bone.consume_accessed()
        {
            f(ChangedKind::Bone)
        }

        if self.muscle.consume_accessed()
        {
            f(ChangedKind::Muscle)
        }

        if self.skin.consume_accessed()
        {
            f(ChangedKind::Skin)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Halves<T>
{
    pub left: T,
    pub right: T
}

impl<T> Halves<T>
{
    pub fn repeat(value: T) -> Self
    where
        T: Clone
    {
        Self{left: value.clone(), right: value}
    }

    pub fn flip(self) -> Self
    {
        Self{
            left: self.right,
            right: self.left
        }
    }

    pub fn as_ref(&self) -> Halves<&T>
    {
        Halves{
            left: &self.left,
            right: &self.right
        }
    }

    pub fn as_mut(&mut self) -> Halves<&mut T>
    {
        Halves{
            left: &mut self.left,
            right: &mut self.right
        }
    }

    pub fn zip<U>(self, other: Halves<U>) -> Halves<(T, U)>
    {
        Halves{
            left: (self.left, other.left),
            right: (self.right, other.right)
        }
    }

    pub fn map_sides<U>(self, mut f: impl FnMut(Side1d, T) -> U) -> Halves<U>
    {
        Halves{
            left: f(Side1d::Left, self.left),
            right: f(Side1d::Right, self.right)
        }
    }

    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Halves<U>
    {
        self.map_sides(|_, x| f(x))
    }

    pub fn combine<U>(self, mut f: impl FnMut(T, T) -> U) -> U
    {
        f(self.left, self.right)
    }
}

impl<T> Index<Side1d> for Halves<T>
{
    type Output = T;

    fn index(&self, side: Side1d) -> &Self::Output
    {
        match side
        {
            Side1d::Left => &self.left,
            Side1d::Right => &self.right
        }
    }
}

impl<T> IndexMut<Side1d> for Halves<T>
{
    fn index_mut(&mut self, side: Side1d) -> &mut Self::Output
    {
        match side
        {
            Side1d::Left => &mut self.left,
            Side1d::Right => &mut self.right
        }
    }
}

impl<T: HealthIterate> HealthIterate for Halves<Option<T>>
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.left.as_ref().map(|x| x.health_iter()).into_iter().flatten()
            .chain(self.right.as_ref().map(|x| x.health_iter()).into_iter().flatten())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        let left_value = self.left.as_mut().map(|x| x.health_sided_iter_mut(side)).into_iter().flatten();
        let right_value = self.right.as_mut().map(|x| x.health_sided_iter_mut(side)).into_iter().flatten();

        match side
        {
            Side2d::Left =>
            {
                left_value.chain(right_value)
            },
            Side2d::Right =>
            {
                right_value.chain(left_value)
            },
            Side2d::Front | Side2d::Back =>
            {
                if fastrand::bool()
                {
                    left_value.chain(right_value)
                } else
                {
                    right_value.chain(left_value)
                }
            }
        }
    }
}

impl HealthIterate for ChangeTracking<Health>
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        iter::once(self)
    }

    fn health_sided_iter_mut(&mut self, _side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        iter::once(self)
    }
}

impl HealReceiver for ChangeTracking<Health> {}
impl DamageReceiver for ChangeTracking<Health> {}

impl Organ for ChangeTracking<Health>
{
    fn average_health(&self) -> Option<f32>
    {
        self.fraction()
    }

    fn size(&self) -> &f64
    {
        unreachable!()
    }

    fn consume_accessed(&mut self) -> bool
    {
        Self::consume_accessed(self)
    }
}
