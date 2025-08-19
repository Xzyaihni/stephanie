use std::{
    f32,
    fmt::{self, Debug, Display},
    ops::{Index, IndexMut, ControlFlow, Deref, DerefMut}
};

use serde::{Serialize, Deserialize};

use crate::{
    debug_config::*,
    common::{
        SeededRandom,
        Damage,
        DamageType,
        Side1d,
        Side2d,
        Damageable
    }
};

pub use human::*;

mod human;


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
simple_field_getter!{MuscleHealthGetter, Option<Health>, muscle}
simple_field_getter!{SkinHealthGetter, Option<Health>, skin}
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
    type V<'a> = f32;

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
        Box::new(|mut damage|
        {
            let data = value.damage(&mut damage.rng, damage.direction.side, damage.data);

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
    simple_getter!(stamina_speed);
    simple_getter!(max_stamina);
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

    pub fn set_speed(&mut self, speed: f32)
    {
        match self
        {
            Self::Human(x) => x.set_speed(speed)
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

fn heal_iterative<const COUNT: usize>(
    amount: f32,
    mut values: [&mut dyn HealReceiver; COUNT]
) -> Option<f32>
{
    let mut pool = amount;

    let mut filled = values.each_ref().map(|x| x.is_full());

    loop
    {
        let mut current = filled.iter().filter(|x| !**x).count();

        if current == 0
        {
            break;
        }

        for (value, filled) in values.iter_mut().zip(filled.iter_mut()).filter(|(_, filled)| !**filled)
        {
            let heal_amount = pool / current as f32;
            pool -= heal_amount;

            pool += value.heal(heal_amount).unwrap_or(0.0);

            if pool <= 0.0
            {
                return None;
            }

            current -= 1;

            if value.is_full()
            {
                *filled = true;
            }
        }
    }

    Some(pool)
}

pub trait HealReceiver
{
    fn is_full(&self) -> bool;
    fn heal(&mut self, amount: f32) -> Option<f32>;
}

pub trait DamageReceiver: HealReceiver
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>;
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

    pub fn subtract_hp(&mut self, amount: f32)
    {
        self.current = (self.current - amount).clamp(0.0, self.max);
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

    pub fn fraction(&self) -> f32
    {
        self.current / self.max
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
    pub max_block: f32,
    pub health: SimpleHealth
}

impl Debug for Health
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "Health {{ ({:.3}) {} }}", self.max_block, self.health)
    }
}

impl Health
{
    pub fn new(max_block: f32, max: f32) -> Self
    {
        Self{max_block, health: SimpleHealth::new(max)}
    }

    pub fn fraction(&self) -> f32
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

    pub fn damage_pierce(&mut self, damage: DamageType) -> Option<DamageType>
    {
        match damage
        {
            DamageType::Blunt(damage) =>
            {
                self.simple_pierce(damage).map(DamageType::Blunt)
            },
            DamageType::Sharp{sharpness, damage} =>
            {
                self.pierce_with(sharpness, damage).map(|damage|
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
        let pass = (damage - self.max_block.min(self.health.current())) * (sharpness + 1.0);
        self.health.subtract_hp(damage);

        if pass <= 0.0
        {
            None
        } else
        {
            Some(pass)
        }
    }

    pub fn pierce_many<T>(
        damage: DamageType,
        mut parts: impl Iterator<Item=T>,
        mut f: impl FnMut(T, DamageType) -> Option<DamageType>
    ) -> Option<DamageType>
    {
        let result = parts.try_fold(damage, |acc, x|
        {
            if let Some(pierce) = f(x, acc)
            {
                ControlFlow::Continue(pierce)
            } else
            {
                ControlFlow::Break(())
            }
        });

        match result
        {
            ControlFlow::Continue(x) => Some(x),
            ControlFlow::Break(_) => None
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
    fn average_health(&self) -> f32;
    fn size(&self) -> &f64;

    fn is_broken(&self) -> bool { self.average_health() <= 0.0 }
    fn consume_accessed(&mut self) -> bool { unimplemented!() }
}

impl HealReceiver for ()
{
    fn is_full(&self) -> bool { true }
    fn heal(&mut self, amount: f32) -> Option<f32> { Some(amount) }
}

impl DamageReceiver for ()
{
    fn damage(
        &mut self,
        _rng: &mut SeededRandom,
        _side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        Some(damage)
    }
}

impl Organ for ()
{
    fn average_health(&self) -> f32 { 0.0 }
    fn size(&self) -> &f64 { &0.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyPart<Contents=()>
{
    name: DebugName,
    pub bone: ChangeTracking<Health>,
    pub skin: ChangeTracking<Option<Health>>,
    pub muscle: ChangeTracking<Option<Health>>,
    size: f64,
    contents: Contents
}

impl<Contents: Organ> HealReceiver for BodyPart<Contents>
{
    fn is_full(&self) -> bool
    {
        self.bone.is_full()
            && self.skin.map(|x| x.is_full()).unwrap_or(true)
            && self.muscle.map(|x| x.is_full()).unwrap_or(true)
            && self.contents.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            &mut self.bone,
            &mut self.skin,
            &mut self.muscle,
            &mut self.contents
        ])
    }
}

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
            Health::new(bone * 0.001, bone),
            Some(Health::new(info.skin_toughness * 0.05, info.skin_toughness)),
            Some(Health::new(info.muscle_toughness * 0.2, info.muscle_toughness * 5.0)),
            size,
            contents
        )
    }

    pub fn new_full(
        name: DebugName,
        bone: Health,
        skin: Option<Health>,
        muscle: Option<Health>,
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
        self.bone.fraction() == 0.0
            && self.skin.map(|x| x.fraction() == 0.0).unwrap_or(true)
            && self.muscle.map(|x| x.fraction() == 0.0).unwrap_or(true)
    }

    fn speed_multiply(&self, base: f32, override_muscle: Option<f32>) -> f32
    {
        let muscle_health = override_muscle.unwrap_or_else(||
        {
            self.muscle.as_ref().map(|x| x.fraction()).unwrap_or(0.0)
        });

        let health_mult = (self.bone.fraction() * 0.9 + 0.1) * muscle_health;

        base * health_mult
    }

    pub fn average_health(&self) -> f32
    {
        let mut count = 0;
        let mut total = 0.0;

        let mut with_total = |value: Option<Health>|
        {
            if let Some(value) = value
            {
                count += 1;
                total += value.fraction();
            }
        };

        with_total(Some(*self.bone));
        with_total(*self.skin);
        with_total(*self.muscle);

        total / count as f32
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

        let mut rng = damage.rng;
        let direction = damage.direction;

        self.damage_inner(&mut rng, direction.side, damage.data).map(|damage|
        {
            Damage{rng, direction, data: damage}
        })
    }

    fn damage_inner(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        // huh
        if let Some(pierce) = self.skin.as_mut().map(|x|
        {
            let base_mult = 0.1;
            match damage
            {
                DamageType::Blunt(_) => x.damage_pierce(damage * base_mult),
                DamageType::Sharp{sharpness, ..} =>
                {
                    x.damage_pierce(damage * (base_mult + sharpness).clamp(0.0, 1.0))
                },
                DamageType::Bullet(_) => x.damage_pierce(damage)
            }
        }).unwrap_or(Some(damage))
        {
            if let Some(pierce) = self.muscle.as_mut().map(|x| x.damage_pierce(damage))
                .unwrap_or(Some(pierce))
            {
                if let Some(pierce) = self.bone.damage_pierce(pierce)
                {
                    return self.contents.damage(rng, side, pierce);
                }
            }
        }

        None
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

impl HealReceiver for ChangeTracking<Health>
{
    fn is_full(&self) -> bool
    {
        Health::is_full(self)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        self.heal_remainder(amount)
    }
}

impl HealReceiver for ChangeTracking<Option<Health>>
{
    fn is_full(&self) -> bool
    {
        self.as_ref().map(|x| x.is_full()).unwrap_or(true)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        self.as_mut().map(|x| x.heal_remainder(amount)).unwrap_or(Some(amount))
    }
}

impl DamageReceiver for ChangeTracking<Health>
{
    fn damage(
        &mut self,
        _rng: &mut SeededRandom,
        _side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.damage_pierce(damage)
    }
}

impl Organ for ChangeTracking<Health>
{
    fn average_health(&self) -> f32
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

#[cfg(test)]
mod tests
{
    use super::*;


    #[test]
    fn healing()
    {
        let health_with = |amount| -> ChangeTracking<Health>
        {
            let mut h = Health::new(fastrand::f32(), 1.0);
            h.health.current = amount;

            h.into()
        };

        let mut a = health_with(0.7);
        let mut b = health_with(0.3);
        let mut c = health_with(0.2);

        heal_iterative(1.0, [&mut a, &mut b, &mut c]);

        let e = f32::EPSILON;

        assert_eq!(a.current(), 1.0);
        assert!((b.current() - 0.65).abs() < e);
        assert!((c.current() - 0.55).abs() < e);
    }
}
