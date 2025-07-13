use std::{
    f32,
    mem,
    rc::Rc,
    fmt::{self, Debug, Display},
    ops::{Index, IndexMut, ControlFlow, Deref, DerefMut}
};

use serde::{Serialize, Deserialize};

use strum::{EnumCount, FromRepr, IntoStaticStr};

use crate::{
    debug_config::*,
    common::{
        some_or_value,
        some_or_return,
        from_upper_camel,
        SeededRandom,
        WeightedPicker,
        Damage,
        DamageHeight,
        DamageType,
        Side1d,
        Side2d,
        Damageable,
        world::TILE_SIZE
    }
};


type DebugName = <DebugConfig as DebugConfigTrait>::DebugName;

macro_rules! simple_getter
{
    ($name:ident) =>
    {
        pub fn $name(&self) -> Option<f32>
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

fn no_zero(value: f32) -> Option<f32>
{
    (value != 0.0).then_some(value)
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
    simple_getter!(stamina);
    simple_getter!(max_stamina);
    simple_getter!(vision);
    simple_getter!(vision_angle);

    pub fn get_human<F>(
        &self,
        id: AnatomyId
    ) -> Option<Option<<F as PartFieldGetter<RefHumanPartFieldGet>>::V<'_>>>
    where
        F: PartFieldGetter<RefHumanPartFieldGet>,
        F: for<'a> PartFieldGetter<RefOrganFieldGet, V<'a>=<F as PartFieldGetter<RefHumanPartFieldGet>>::V<'a>>
    {
        let human = self.as_human()?;
        Some(human.body.get::<F>(id))
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

    pub fn is_crawling(&self) -> bool
    {
        match self
        {
            Self::Human(x) => x.is_crawling()
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeTracking<T>
{
    accessed: bool,
    value: T
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
            Health::new(bone * 0.05, bone),
            Some(Health::new(info.skin_toughness * 5.0, info.skin_toughness * 100.0)),
            Some(Health::new(info.muscle_toughness * 20.0, info.muscle_toughness * 500.0)),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotorCortex
{
    arms: ChangeTracking<Health>,
    body: ChangeTracking<Health>,
    legs: ChangeTracking<Health>
}

impl Default for MotorCortex
{
    fn default() -> Self
    {
        Self{
            arms: Health::new(4.0, 50.0).into(),
            body: Health::new(4.0, 50.0).into(),
            legs: Health::new(4.0, 50.0).into()
        }
    }
}

impl HealReceiver for MotorCortex
{
    fn is_full(&self) -> bool
    {
        self.arms.is_full() && self.body.is_full() && self.legs.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [&mut self.arms, &mut self.body, &mut self.legs])
    }
}

impl DamageReceiver for MotorCortex
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        let mut order = vec![&mut self.arms, &mut self.body, &mut self.legs];

        match side
        {
            Side2d::Left | Side2d::Right =>
            {
                let order = if rng.next_bool()
                {
                    order.into_iter().rev().collect()
                } else
                {
                    order
                };

                Health::pierce_many(damage, order.into_iter(), |part, damage|
                {
                    part.damage_pierce(damage)
                })
            },
            Side2d::Front | Side2d::Back =>
            {
                let len = order.len();
                order[rng.next_usize_between(0..len)].damage_pierce(damage)
            }
        }
    }
}

impl Organ for MotorCortex
{
    fn average_health(&self) -> f32
    {
        (self.arms.fraction() + self.body.fraction() + self.legs.fraction()) / 3.0
    }

    fn size(&self) -> &f64
    {
        &0.05
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontalLobe
{
    motor: MotorCortex
}

#[allow(clippy::derivable_impls)]
impl Default for FrontalLobe
{
    fn default() -> Self
    {
        Self{motor: MotorCortex::default()}
    }
}

impl HealReceiver for FrontalLobe
{
    fn is_full(&self) -> bool
    {
        self.motor.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        self.motor.heal(amount)
    }
}

impl DamageReceiver for FrontalLobe
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.motor.damage(rng, side, damage)
    }
}

impl Organ for FrontalLobe
{
    fn average_health(&self) -> f32
    {
        self.motor.average_health()
    }

    fn size(&self) -> &f64
    {
        &0.05
    }
}

#[derive(Debug, Clone, Copy, FromRepr, EnumCount, Serialize, Deserialize)]
pub enum LobeId
{
    Frontal,
    Parietal,
    Temporal,
    Occipital
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParietalLobe(ChangeTracking<Health>);

impl Default for ParietalLobe
{
    fn default() -> Self
    {
        Self(Health::new(4.0, 50.0).into())
    }
}

impl HealReceiver for ParietalLobe
{
    fn is_full(&self) -> bool
    {
        self.0.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        self.0.heal(amount)
    }
}

impl DamageReceiver for ParietalLobe
{
    fn damage(
        &mut self,
        _rng: &mut SeededRandom,
        _side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.0.damage_pierce(damage)
    }
}

impl Organ for ParietalLobe
{
    fn average_health(&self) -> f32
    {
        self.0.fraction()
    }

    fn size(&self) -> &f64
    {
        &0.01
    }

    fn consume_accessed(&mut self) -> bool
    {
        self.0.consume_accessed()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalLobe(ChangeTracking<Health>);

impl Default for TemporalLobe
{
    fn default() -> Self
    {
        Self(Health::new(4.0, 50.0).into())
    }
}

impl HealReceiver for TemporalLobe
{
    fn is_full(&self) -> bool
    {
        self.0.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        self.0.heal(amount)
    }
}

impl DamageReceiver for TemporalLobe
{
    fn damage(
        &mut self,
        _rng: &mut SeededRandom,
        _side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.0.damage_pierce(damage)
    }
}

impl Organ for TemporalLobe
{
    fn average_health(&self) -> f32
    {
        self.0.fraction()
    }

    fn size(&self) -> &f64
    {
        &0.01
    }

    fn consume_accessed(&mut self) -> bool
    {
        self.0.consume_accessed()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OccipitalLobe(ChangeTracking<Health>);

impl Default for OccipitalLobe
{
    fn default() -> Self
    {
        Self(Health::new(4.0, 50.0).into())
    }
}

impl HealReceiver for OccipitalLobe
{
    fn is_full(&self) -> bool
    {
        self.0.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        self.0.heal(amount)
    }
}

impl DamageReceiver for OccipitalLobe
{
    fn damage(
        &mut self,
        _rng: &mut SeededRandom,
        _side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.0.damage_pierce(damage)
    }
}

impl Organ for OccipitalLobe
{
    fn average_health(&self) -> f32
    {
        self.0.fraction()
    }

    fn size(&self) -> &f64
    {
        &0.01
    }

    fn consume_accessed(&mut self) -> bool
    {
        self.0.consume_accessed()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hemisphere
{
    frontal: FrontalLobe,
    parietal: ParietalLobe,
    temporal: TemporalLobe,
    occipital: OccipitalLobe
}

impl Default for Hemisphere
{
    fn default() -> Self
    {
        Self{
            frontal: FrontalLobe::default(),
            parietal: ParietalLobe::default(),
            temporal: TemporalLobe::default(),
            occipital: OccipitalLobe::default()
        }
    }
}

impl Hemisphere
{
    fn damage_lobe(
        &mut self,
        lobe: LobeId,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        match lobe
        {
            LobeId::Frontal => self.frontal.damage(rng, side, damage),
            LobeId::Parietal => self.parietal.damage(rng, side, damage),
            LobeId::Temporal => self.temporal.damage(rng, side, damage),
            LobeId::Occipital => self.occipital.damage(rng, side, damage)
        }
    }
}

impl HealReceiver for Hemisphere
{
    fn is_full(&self) -> bool
    {
        self.frontal.is_full() && self.parietal.is_full() && self.temporal.is_full() && self.occipital.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [&mut self.frontal, &mut self.parietal, &mut self.temporal, &mut self.occipital])
    }
}

impl DamageReceiver for Hemisphere
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        let middle = if rng.next_bool()
        {
            LobeId::Parietal
        } else
        {
            LobeId::Temporal
        };

        let order = match side
        {
            Side2d::Left | Side2d::Right =>
            {
                let lobe = LobeId::from_repr(rng.next_usize_between(0..LobeId::COUNT)).unwrap();

                return self.damage_lobe(lobe, rng, side, damage);
            },
            Side2d::Front =>
            {
                [LobeId::Frontal, middle, LobeId::Occipital]
            },
            Side2d::Back =>
            {
                [LobeId::Occipital, middle, LobeId::Frontal]
            }
        };

        Health::pierce_many(damage, order.into_iter(), |id, damage|
        {
            self.damage_lobe(id, rng, side, damage)
        })
    }
}

impl Organ for Hemisphere
{
    fn average_health(&self) -> f32
    {
        (self.frontal.average_health()
            + self.parietal.average_health()
            + self.temporal.average_health()
            + self.occipital.average_health()) / 4.0
    }

    fn size(&self) -> &f64
    {
        &0.1
    }
}

pub type Brain = Halves<Hemisphere>;

impl Default for Brain
{
    fn default() -> Self
    {
        Self{left: Hemisphere::default(), right: Hemisphere::default()}
    }
}

impl HealReceiver for Brain
{
    fn is_full(&self) -> bool
    {
        self.left.is_full() && self.right.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [&mut self.left, &mut self.right])
    }
}

impl DamageReceiver for Brain
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        let hemispheres = [&mut self.left, &mut self.right];

        match side
        {
            Side2d::Left =>
            {
                Health::pierce_many(damage, hemispheres.into_iter(), |part, damage|
                {
                    part.damage(rng, side, damage)
                })
            },
            Side2d::Right =>
            {
                Health::pierce_many(damage, hemispheres.into_iter().rev(), |part, damage|
                {
                    part.damage(rng, side, damage)
                })
            },
            Side2d::Front | Side2d::Back =>
            {
                if rng.next_bool()
                {
                    hemispheres[0].damage(rng, side, damage)
                } else
                {
                    hemispheres[1].damage(rng, side, damage)
                }
            }
        }
    }
}

impl Organ for Brain
{
    fn average_health(&self) -> f32
    {
        self.as_ref().map(|x| x.average_health()).combine(|a, b| (a + b) / 2.0)
    }

    fn size(&self) -> &f64
    {
        &0.2
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eye
{
    health: ChangeTracking<Health>
}

impl Eye
{
    pub fn new() -> Self
    {
        Self{health: Health::new(50.0, 100.0).into()}
    }
}

impl HealReceiver for Eye
{
    fn is_full(&self) -> bool
    {
        self.health.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        self.health.heal(amount)
    }
}

impl DamageReceiver for Eye
{
    fn damage(
        &mut self,
        _rng: &mut SeededRandom,
        _side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.health.damage_pierce(damage)
    }
}

impl Organ for Eye
{
    fn average_health(&self) -> f32
    {
        self.health.fraction()
    }

    fn size(&self) -> &f64
    {
        &0.1
    }

    fn consume_accessed(&mut self) -> bool
    {
        self.health.consume_accessed()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lung
{
    health: ChangeTracking<Health>
}

impl Lung
{
    fn new() -> Self
    {
        Self{health: Health::new(3.0, 20.0).into()}
    }
}

impl HealReceiver for Lung
{
    fn is_full(&self) -> bool
    {
        self.health.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        self.health.heal(amount)
    }
}

impl DamageReceiver for Lung
{
    fn damage(
        &mut self,
        _rng: &mut SeededRandom,
        _side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.health.damage_pierce(damage)
    }
}

impl Organ for Lung
{
    fn average_health(&self) -> f32
    {
        self.health.fraction()
    }

    fn size(&self) -> &f64
    {
        &0.3
    }

    fn consume_accessed(&mut self) -> bool
    {
        self.health.consume_accessed()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MotorId
{
    Arms,
    Body,
    Legs
}

impl Display for MotorId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Arms => write!(f, "arms"),
            Self::Body => write!(f, "body"),
            Self::Legs => write!(f, "legs")
        }
    }
}

impl MotorId
{
    pub fn iter() -> impl Iterator<Item=Self>
    {
        [
            Self::Arms,
            Self::Body,
            Self::Legs
        ].into_iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrontalId
{
    Motor(MotorId)
}

impl Display for FrontalId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Motor(id) => write!(f, "motor cortex ({id} muscle group)")
        }
    }
}

impl FrontalId
{
    pub fn iter() -> impl Iterator<Item=Self>
    {
        MotorId::iter().map(Self::Motor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrainId
{
    Frontal(FrontalId),
    Parietal,
    Temporal,
    Occipital
}

impl Display for BrainId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Frontal(id) => return Display::fmt(id, f),
            _ => ()
        }

        let name = match self
        {
            Self::Parietal => "parietal",
            Self::Temporal => "temporal",
            Self::Occipital => "occipital",
            x => unreachable!("{x:?}")
        };

        write!(f, "{name} cortex")
    }
}

impl BrainId
{
    pub fn iter() -> impl Iterator<Item=Self>
    {
        [
            Self::Parietal,
            Self::Temporal,
            Self::Occipital
        ].into_iter().chain(FrontalId::iter().map(Self::Frontal))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrganId
{
    Eye(Side1d),
    Brain(Option<Side1d>, Option<BrainId>),
    Lung(Side1d)
}

impl Display for OrganId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let (side, name) = match self
        {
            Self::Eye(side) => (side, "eye".to_owned()),
            Self::Lung(side) => (side, "lung".to_owned()),
            Self::Brain(side, id) =>
            {
                let name = id.map(|x| x.to_string()).unwrap_or_else(|| "hemisphere".to_owned());

                if let Some(side) = side
                {
                    (side, name)
                } else
                {
                    return write!(f, "{name}");
                }
            }
        };

        write!(f, "{side} {name}")
    }
}

impl OrganId
{
    pub fn iter() -> impl Iterator<Item=Self>
    {
        [
            Self::Eye(Side1d::Left),
            Self::Eye(Side1d::Right),
            Self::Lung(Side1d::Left),
            Self::Lung(Side1d::Right)
        ].into_iter()
            .chain(BrainId::iter().map(|id| Self::Brain(Some(Side1d::Left), Some(id))))
            .chain(BrainId::iter().map(|id| Self::Brain(Some(Side1d::Right), Some(id))))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnatomyId
{
    Organ(OrganId),
    Part(HumanPartId)
}

impl From<OrganId> for AnatomyId
{
    fn from(id: OrganId) -> Self
    {
        Self::Organ(id)
    }
}

impl From<HumanPartId> for AnatomyId
{
    fn from(id: HumanPartId) -> Self
    {
        Self::Part(id)
    }
}

impl AnatomyId
{
    pub fn iter() -> impl Iterator<Item=Self>
    {
        HumanPartId::iter().map(Self::Part)
            .chain(OrganId::iter().map(Self::Organ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr, Serialize, Deserialize)]
pub enum HumanPartId
{
    Head,
    Torso,
    Spine,
    Pelvis,
    Thigh(Side1d),
    Calf(Side1d),
    Arm(Side1d),
    Forearm(Side1d),
    Hand(Side1d),
    Foot(Side1d)
}

impl Display for HumanPartId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let maybe_side: String = self.side_name();

        let s: &str = self.into();
        let name = from_upper_camel(s);

        write!(f, "{maybe_side}{name}")
    }
}

impl HumanPartId
{
    pub fn side(&self) -> Option<Side1d>
    {
        match self
        {
            Self::Thigh(x)
            | Self::Calf(x)
            | Self::Arm(x)
            | Self::Forearm(x)
            | Self::Hand(x)
            | Self::Foot(x) => Some(*x),
            _ => None
        }
    }

    pub fn iter() -> impl Iterator<Item=Self>
    {
        [
            Self::Head,
            Self::Torso,
            Self::Spine,
            Self::Pelvis,
            Self::Thigh(Side1d::Left),
            Self::Thigh(Side1d::Right),
            Self::Calf(Side1d::Left),
            Self::Calf(Side1d::Right),
            Self::Arm(Side1d::Left),
            Self::Arm(Side1d::Right),
            Self::Forearm(Side1d::Left),
            Self::Forearm(Side1d::Right),
            Self::Hand(Side1d::Left),
            Self::Hand(Side1d::Right),
            Self::Foot(Side1d::Left),
            Self::Foot(Side1d::Right)
        ].into_iter()
    }

    pub fn bone_to_string(&self) -> String
    {
        let maybe_side = self.side_name();

        let name = self.bone_name();

        format!("{maybe_side}{name}")
    }

    pub fn bone_name(&self) -> &str
    {
        match self
        {
            Self::Head => "skull",
            Self::Torso => "ribcage",
            Self::Pelvis => "pelvis",
            Self::Spine => "spine",
            Self::Thigh(_) => "femur",
            Self::Calf(_) => "tibia",
            Self::Arm(_) => "humerus",
            Self::Forearm(_) => "radius",
            Self::Hand(_) => "hand bones",
            Self::Foot(_) => "foot bones"
        }
    }

    pub fn side_name(&self) -> String
    {
        self.side().map(|s|
        {
            format!("{s} ")
        }).unwrap_or_default()
    }
}

pub type HumanPart<Contents=()> = BodyPart<Contents>;

#[derive(Debug, Default, Clone)]
struct Speeds
{
    arms: f32,
    legs: f32
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CachedProps
{
    speed: Option<f32>,
    is_crawling: bool,
    strength: Option<f32>,
    stamina: Option<f32>,
    max_stamina: Option<f32>,
    vision: Option<f32>,
    blood_change: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanAnatomyInfo
{
    pub bone_toughness: f32,
    pub muscle_toughness: f32,
    pub skin_toughness: f32,
    pub base_speed: f32,
    pub base_strength: f32
}

impl Default for HumanAnatomyInfo
{
    fn default() -> Self
    {
        Self{
            bone_toughness: 1.0,
            muscle_toughness: 1.0,
            skin_toughness: 1.0,
            base_speed: 1.0,
            base_strength: 1.0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadOrgans
{
    pub eyes: Halves<Option<Eye>>,
    pub brain: Option<Brain>
}

impl HealReceiver for HeadOrgans
{
    fn is_full(&self) -> bool
    {
        self.eyes.as_ref().map(|x| x.as_ref().map(|x| x.is_full()).unwrap_or(true)).combine(|a, b| a && b)
            && self.brain.as_ref().map(|x| x.is_full()).unwrap_or(true)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            self.eyes.left.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.eyes.right.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.brain.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ())
        ])
    }
}

impl DamageReceiver for HeadOrgans
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        if let Some(brain) = self.brain.as_mut()
        {
            brain.damage(rng, side, damage)
        } else
        {
            Some(damage)
        }
    }
}

impl Organ for HeadOrgans
{
    fn average_health(&self) -> f32
    {
        unimplemented!()
    }

    fn size(&self) -> &f64
    {
        unimplemented!()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorsoOrgans
{
    pub lungs: Halves<Option<Lung>>
}

impl Organ for TorsoOrgans
{
    fn average_health(&self) -> f32
    {
        unimplemented!()
    }

    fn size(&self) -> &f64
    {
        unimplemented!()
    }
}

impl HealReceiver for TorsoOrgans
{
    fn is_full(&self) -> bool
    {
        self.lungs.as_ref().map(|x| x.as_ref().map(|x| x.is_full()).unwrap_or(true)).combine(|a, b| a && b)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            self.lungs.left.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.lungs.right.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ())
        ])
    }
}

impl DamageReceiver for TorsoOrgans
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        let exists = self.lungs.as_ref().map(|x| x.is_some());
        if exists.clone().combine(|a, b| !a && !b)
        {
            return Some(damage);
        }

        let lung = if exists.clone().combine(|a, b| a && b)
        {
            if rng.next_bool()
            {
                &mut self.lungs.left
            } else
            {
                &mut self.lungs.right
            }
        } else
        {
            if exists.left
            {
                &mut self.lungs.left
            } else
            {
                &mut self.lungs.right
            }
        };

        lung.as_mut().unwrap().damage(rng, side, damage)
    }
}

macro_rules! remove_broken
{
    ($this:expr, $on_break:expr $(, $part:ident)?) =>
    {
        let is_broken = $this.as_ref().map(|x|
        {
            x$(.$part)?.is_broken()
        }).unwrap_or(false);

        if is_broken
        {
            $this.take();
            $on_break();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LowerLimb
{
    lower: HumanPart,
    leaf: Option<HumanPart>
}

impl HealReceiver for LowerLimb
{
    fn is_full(&self) -> bool
    {
        self.lower.is_full()
            && self.leaf.as_ref().map(|x| x.is_full()).unwrap_or(true)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            &mut self.lower,
            self.leaf.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ())
        ])
    }
}

impl LowerLimb
{
    fn detach_broken(&mut self, on_break: impl FnOnce())
    {
        if self.leaf.as_ref().map(|x| x.is_broken()).unwrap_or(false)
        {
            self.leaf = None;
            on_break();
        }
    }

    fn speed_with(&self, lower: f32, leaf: f32) -> f32
    {
        let leaf_speed = self.leaf.as_ref().map(|x|
        {
            let muscle = self.lower.muscle.map(|x| x.fraction()).unwrap_or(0.0);
            x.speed_multiply(leaf, Some(muscle))
        }).unwrap_or_default();

        self.lower.speed_multiply(lower, None) + leaf_speed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limb
{
    upper: HumanPart,
    lower: Option<LowerLimb>
}

impl HealReceiver for Limb
{
    fn is_full(&self) -> bool
    {
        self.upper.is_full()
            && self.lower.as_ref().map(|x| x.is_full()).unwrap_or(true)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            &mut self.upper,
            self.lower.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ())
        ])
    }
}

impl Limb
{
    fn detach_broken<OnBreak: FnMut(AnatomyId)>(
        &mut self,
        on_break: &mut OnBreak,
        on_lower: impl FnOnce(&mut OnBreak),
        on_leaf: impl FnOnce(&mut OnBreak)
    )
    {
        remove_broken!(self.lower, || on_lower(on_break), lower);

        if let Some(lower) = self.lower.as_mut()
        {
            lower.detach_broken(|| on_leaf(on_break));
        }
    }

    fn speed_with(&self, upper: f32, lower: f32, leaf: f32) -> f32
    {
        self.upper.speed_multiply(upper, None)
            + self.lower.as_ref().map(|x| x.speed_with(lower, leaf)).unwrap_or_default()
    }

    fn arm_speed(&self) -> f32
    {
        self.speed_with(0.2, 0.1, 0.05)
    }

    fn leg_speed(&self) -> f32
    {
        self.speed_with(0.4, 0.12, 0.07)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Torso
{
    pub torso: HumanPart<TorsoOrgans>,
    pub arms: Halves<Option<Limb>>
}

impl HealReceiver for Torso
{
    fn is_full(&self) -> bool
    {
        self.torso.is_full()
            && self.arms.as_ref().map(|x| x.as_ref().map(|x| x.is_full()).unwrap_or(true)).combine(|a, b| a && b)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            &mut self.torso,
            self.arms.left.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.arms.right.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ())
        ])
    }
}

impl Torso
{
    fn detach_broken(&mut self, on_break: &mut impl FnMut(AnatomyId))
    {
        self.torso.contents.lungs.as_mut().map_sides(|side, lung|
        {
            remove_broken!(lung, || on_break(AnatomyId::Organ(OrganId::Lung(side))));
        });

        self.arms.as_mut().map_sides(|side, arm|
        {
            remove_broken!(arm.as_mut(), || on_break(AnatomyId::Part(HumanPartId::Arm(side))), upper);

            if let Some(arm) = arm.as_mut()
            {
                arm.detach_broken(
                    on_break,
                    |on_break| on_break(AnatomyId::Part(HumanPartId::Forearm(side))),
                    |on_break| on_break(AnatomyId::Part(HumanPartId::Hand(side)))
                );
            }
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pelvis
{
    pub pelvis: HumanPart,
    pub legs: Halves<Option<Limb>>
}

impl HealReceiver for Pelvis
{
    fn is_full(&self) -> bool
    {
        self.pelvis.is_full()
            && self.legs.as_ref().map(|x| x.as_ref().map(|x| x.is_full()).unwrap_or(true)).combine(|a, b| a && b)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            &mut self.pelvis,
            self.legs.left.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.legs.right.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ())
        ])
    }
}

impl Pelvis
{
    fn detach_broken(&mut self, on_break: &mut impl FnMut(AnatomyId))
    {
        self.legs.as_mut().map_sides(|side, leg|
        {
            remove_broken!(leg.as_mut(), || on_break(AnatomyId::Part(HumanPartId::Thigh(side))), upper);

            if let Some(leg) = leg.as_mut()
            {
                leg.detach_broken(
                    on_break,
                    |on_break| on_break(AnatomyId::Part(HumanPartId::Calf(side))),
                    |on_break| on_break(AnatomyId::Part(HumanPartId::Foot(side)))
                );
            }
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spine
{
    pub spine: HumanPart,
    pub torso: Option<Torso>,
    pub pelvis: Option<Pelvis>
}

impl HealReceiver for Spine
{
    fn is_full(&self) -> bool
    {
        self.spine.is_full()
            && self.torso.as_ref().map(|x| x.is_full()).unwrap_or(true)
            && self.pelvis.as_ref().map(|x| x.is_full()).unwrap_or(true)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            &mut self.spine,
            self.torso.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.pelvis.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ())
        ])
    }
}

impl Spine
{
    fn detach_broken(&mut self, on_break: &mut impl FnMut(AnatomyId))
    {
        remove_broken!(self.torso, || on_break(AnatomyId::Part(HumanPartId::Torso)), torso);
        remove_broken!(self.pelvis, || on_break(AnatomyId::Part(HumanPartId::Pelvis)), pelvis);

        if let Some(torso) = self.torso.as_mut()
        {
            torso.detach_broken(on_break);
        }

        if let Some(pelvis) = self.pelvis.as_mut()
        {
            pelvis.detach_broken(on_break);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanBody
{
    pub head: Option<HumanPart<HeadOrgans>>,
    pub spine: Option<Spine>
}

impl HealReceiver for HumanBody
{
    fn is_full(&self) -> bool
    {
        self.head.as_ref().map(|x| x.is_full()).unwrap_or(true)
            && self.spine.as_ref().map(|x| x.is_full()).unwrap_or(true)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            self.head.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.spine.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ())
        ])
    }
}

impl HumanBody
{
    fn detach_broken(&mut self, mut on_break: impl FnMut(AnatomyId))
    {
        let on_break = &mut on_break;

        remove_broken!(self.head, || on_break(AnatomyId::Part(HumanPartId::Head)));

        if let Some(head) = self.head.as_mut()
        {
            remove_broken!(head.contents.brain, || on_break(AnatomyId::Organ(OrganId::Brain(None, None))));

            head.contents.eyes.as_mut().map_sides(|side, eye|
            {
                remove_broken!(eye, || on_break(AnatomyId::Organ(OrganId::Eye(side))));
            });
        }

        remove_broken!(self.spine, || on_break(AnatomyId::Part(HumanPartId::Spine)), spine);

        if let Some(spine) = self.spine.as_mut()
        {
            spine.detach_broken(on_break);
        }
    }
}

macro_rules! impl_get
{
    (
        $part_getter:ident,
        $organ_getter:ident,
        $fn_name:ident,
        $part_fn_name:ident,
        $organ_fn_name:ident,
        $option_fn:ident,
        $($b:tt)+
    ) =>
    {
        pub fn $fn_name<F>(
            $($b)+ self,
            id: AnatomyId
        ) -> Option<<F as PartFieldGetter<$part_getter>>::V<'_>>
        where
            F: PartFieldGetter<$part_getter>,
            F: for<'a> PartFieldGetter<$organ_getter, V<'a>=<F as PartFieldGetter<$part_getter>>::V<'a>>
        {
            match id
            {
                AnatomyId::Organ(id) => self.$organ_fn_name::<F>(id),
                AnatomyId::Part(id) => self.$part_fn_name::<F>(id)
            }
        }

        pub fn $organ_fn_name<F: PartFieldGetter<$organ_getter>>(
            $($b)+ self,
            id: OrganId
        ) -> Option<F::V<'_>>
        {
            match id
            {
                OrganId::Brain(side, id) =>
                {
                    self.head.$option_fn()?.contents.brain.$option_fn().map(|x|
                    {
                        let side = if let Some(x) = side
                        {
                            x
                        } else
                        {
                            return F::get(x);
                        };

                        let hemisphere = $($b)+ x[side];

                        let id = if let Some(x) = id
                        {
                            x
                        } else
                        {
                            return F::get(hemisphere);
                        };

                        match id
                        {
                            BrainId::Frontal(id) =>
                            {
                                let lobe = $($b)+ hemisphere.frontal;
                                match id
                                {
                                    FrontalId::Motor(id) =>
                                    {
                                        let motor = $($b)+ lobe.motor;
                                        match id
                                        {
                                            MotorId::Arms => F::get($($b)+ motor.arms),
                                            MotorId::Body => F::get($($b)+ motor.body),
                                            MotorId::Legs => F::get($($b)+ motor.legs)
                                        }
                                    }
                                }
                            },
                            BrainId::Parietal => F::get($($b)+ hemisphere.parietal),
                            BrainId::Temporal => F::get($($b)+ hemisphere.temporal),
                            BrainId::Occipital => F::get($($b)+ hemisphere.occipital)
                        }
                    })
                },
                OrganId::Eye(side) =>
                {
                    self.head.$option_fn()?.contents.eyes[side].$option_fn().map(|x| F::get(x))
                },
                OrganId::Lung(side) =>
                {
                    self.spine.$option_fn()
                        .and_then(|x| x.torso.$option_fn())
                        .and_then(|x| x.torso.contents.lungs[side].$option_fn())
                        .map(|x| F::get(x))
                }
            }
        }

        pub fn $part_fn_name<F: PartFieldGetter<$part_getter>>(
            $($b)+ self,
            id: HumanPartId
        ) -> Option<F::V<'_>>
        {
            let spine = self.spine.$option_fn();

            match id
            {
                HumanPartId::Head => return Some(F::get(self.head.$option_fn()?)),
                HumanPartId::Spine => return Some(F::get($($b)+ spine?.spine)),
                _ => ()
            }

            let spine = spine?;

            let torso = spine.torso.$option_fn();
            let pelvis = spine.pelvis.$option_fn();

            match id
            {
                HumanPartId::Head => unreachable!(),
                HumanPartId::Spine => unreachable!(),
                HumanPartId::Torso => Some(F::get($($b)+ torso?.torso)),
                HumanPartId::Pelvis => Some(F::get($($b)+ pelvis?.pelvis)),
                HumanPartId::Thigh(side) => Some(F::get($($b)+ pelvis?.legs[side].$option_fn()?.upper)),
                HumanPartId::Calf(side) => Some(F::get($($b)+ pelvis?.legs[side].$option_fn()?.lower.$option_fn()?.lower)),
                HumanPartId::Foot(side) => Some(F::get(pelvis?.legs[side].$option_fn()?.lower.$option_fn()?.leaf.$option_fn()?)),
                HumanPartId::Arm(side) => Some(F::get($($b)+ torso?.arms[side].$option_fn()?.upper)),
                HumanPartId::Forearm(side) => Some(F::get($($b)+ torso?.arms[side].$option_fn()?.lower.$option_fn()?.lower)),
                HumanPartId::Hand(side) => Some(F::get(torso?.arms[side].$option_fn()?.lower.$option_fn()?.leaf.$option_fn()?))
            }
        }
    }
}

impl HumanBody
{
    impl_get!{RefHumanPartFieldGet, RefOrganFieldGet, get, get_part, get_organ, as_ref, &}
    impl_get!{RefMutHumanPartFieldGet, RefMutOrganFieldGet, get_mut, get_part_mut, get_organ_mut, as_mut, &mut}
}

struct PierceType
{
    possible: Vec<AnatomyId>,
    action: Rc<dyn Fn(&mut HumanAnatomy, Damage) -> Option<Damage>>
}

impl PierceType
{
    fn empty() -> Self
    {
        Self{possible: Vec::new(), action: Rc::new(|_, damage| { Some(damage) })}
    }

    fn always(id: AnatomyId) -> Self
    {
        Self{
            possible: vec![id],
            action: Rc::new(move |this, damage|
            {
                #[allow(clippy::question_mark)]
                if this.body.get::<()>(id).is_none()
                {
                    return None;
                }

                this.body.get_mut::<DamagerGetter>(id).unwrap()(damage)
            })
        }
    }

    fn no_follow() -> fn(&mut HumanAnatomy, Option<Damage>) -> Option<Damage>
    {
        |_this, damage|
        {
            damage
        }
    }

    fn pelvis(side: Side1d) -> Self
    {
        Self::always(HumanPartId::Thigh(side.opposite()).into())
    }

    fn head_back() -> Self
    {
        let possible = vec![OrganId::Eye(Side1d::Left).into(), OrganId::Eye(Side1d::Right).into()];

        Self::possible_pierce(possible, 1, Self::no_follow())
    }

    fn torso_front() -> Self
    {
        Self::possible_pierce(vec![HumanPartId::Spine.into()], 2, Self::no_follow())
    }

    fn possible_pierce<F>(possible: Vec<AnatomyId>, misses: usize, f: F) -> Self
    where
        F: Fn(&mut HumanAnatomy, Option<Damage>) -> Option<Damage> + 'static
    {
        let possible_cloned = possible.clone();

        Self{
            possible,
            action: Rc::new(move |this: &mut HumanAnatomy, mut damage|
            {
                let mut possible_actions = possible_cloned.clone();
                possible_actions.retain(|x| this.body.get::<()>(*x).is_some());

                if possible_actions.is_empty()
                {
                    return f(this, None);
                }

                let miss_check = damage.rng.next_usize_between(0..possible_actions.len() + misses);
                if miss_check >= possible_actions.len()
                {
                    return f(this, None);
                }

                let target = damage.rng.choice(possible_actions);

                let pierce = this.body.get_mut::<DamagerGetter>(target).unwrap()(damage);

                f(this, pierce)
            })
        }
    }

    fn middle_pierce(side: Side1d) -> PierceType
    {
        let opposite = side.opposite();

        let possible = vec![
            HumanPartId::Arm(opposite).into(),
            HumanPartId::Forearm(opposite).into(),
            HumanPartId::Hand(opposite).into()
        ];

        Self::possible_pierce(possible, 0, Self::no_follow())
    }

    fn arm_pierce(side: Side1d) -> PierceType
    {
        let possible = vec![HumanPartId::Spine.into(), HumanPartId::Torso.into()];

        Self::possible_pierce(possible, 0, move |this, pierce|
        {
            (Self::middle_pierce(side).action)(this, pierce?)
        })
    }

    fn leg_pierce(side: Side1d) -> PierceType
    {
        let opposite = side.opposite();

        let possible = vec![
            HumanPartId::Thigh(opposite).into(),
            HumanPartId::Calf(opposite).into(),
            HumanPartId::Foot(opposite).into()
        ];

        Self::possible_pierce(possible, 0, Self::no_follow())
    }

    fn any_exists(&self, anatomy: &HumanAnatomy) -> bool
    {
        self.possible.iter().any(|x| anatomy.body.get::<()>(*x).is_some())
    }

    fn combined_scale(&self, anatomy: &HumanAnatomy) -> f64
    {
        self.possible.iter().filter_map(|x| anatomy.body.get::<SizeGetter>(*x)).sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanAnatomy
{
    base_speed: f32,
    base_strength: f32,
    override_crawling: bool,
    blood: SimpleHealth,
    body: HumanBody,
    broken: Vec<AnatomyId>,
    killed: Option<bool>,
    cached: CachedProps
}

impl Default for HumanAnatomy
{
    fn default() -> Self
    {
        Self::new(HumanAnatomyInfo::default())
    }
}

impl HumanAnatomy
{
    pub fn new(mut info: HumanAnatomyInfo) -> Self
    {
        info.bone_toughness *= 0.3;
        info.muscle_toughness *= 0.6;
        info.skin_toughness *= 0.6;

        let bone_toughness = info.bone_toughness;
        let base_speed = info.base_speed;
        let base_strength = info.base_strength * 2.0;
        let part = BodyPartInfo::from(info);

        fn new_part_with_contents<Contents>(
            name: DebugName,
            part: BodyPartInfo,
            bone_toughness: f32,
            health: f32,
            size: f64,
            contents: Contents
        ) -> HumanPart<Contents>
        {
            HumanPart::new(
                name,
                part,
                bone_toughness * health,
                size,
                contents
            )
        }

        let new_part = |name, health, size|
        {
            new_part_with_contents(name, part.clone(), bone_toughness, health, size, ())
        };

        // max hp is amount of newtons i found on the interner needed to break a bone
        // like half of them i just made up

        let with_name = |side_name|
        {
            move |name|
            {
                format!("{side_name} {name}")
            }
        };

        let make_leg = |side_name|
        {
            let with_name = with_name(side_name);

            let upper = new_part(DebugName::new(with_name("upper leg")), 4000.0, 0.6);
            let lower = new_part(DebugName::new(with_name("lower leg")), 3500.0, 0.44);
            let foot = {
                let mut x = new_part(DebugName::new(with_name("foot")), 2000.0, 0.17);
                x.muscle = None.into();

                Some(x)
            };

            Some(Limb{
                upper,
                lower: Some(LowerLimb{
                    lower,
                    leaf: foot
                })
            })
        };

        let make_arm = |side_name|
        {
            let with_name = with_name(side_name);

            let upper = new_part(DebugName::new(with_name("upper arm")), 2500.0, 0.2);
            let lower = new_part(DebugName::new(with_name("lower arm")), 2000.0, 0.17);
            let hand = {
                let mut x = new_part(DebugName::new(with_name("hand")), 2000.0, 0.07);
                x.muscle = None.into();

                Some(x)
            };


            Some(Limb{
                upper,
                lower: Some(LowerLimb{
                    lower,
                    leaf: hand
                })
            })
        };

        // the spine is very complex sizing wise so im just gonna pick a low-ish number
        let spine = new_part(DebugName::new("spine"), 3400.0, 0.25);

        let head = new_part_with_contents(
            DebugName::new("head"),
            part.clone(),
            bone_toughness,
            5000.0,
            0.39,
            HeadOrgans{eyes: Halves::repeat(Some(Eye::new())), brain: Some(Brain::default())}
        );

        let pelvis = new_part(DebugName::new("pelvis"), 6000.0, 0.37);

        let pelvis = Pelvis{
            pelvis,
            legs: Halves{left: make_leg("left"), right: make_leg("right")}
        };

        let torso = new_part_with_contents(
            DebugName::new("torso"),
            part.clone(),
            bone_toughness,
            3300.0,
            0.82,
            TorsoOrgans{
                lungs: Halves::repeat(Some(Lung::new()))
            }
        );

        let torso = Torso{
            torso,
            arms: Halves{left: make_arm("left"), right: make_arm("right")}
        };

        let spine = Spine{
            spine,
            torso: Some(torso),
            pelvis: Some(pelvis)
        };

        let body = HumanBody{
            head: Some(head),
            spine: Some(spine)
        };

        let mut this = Self{
            base_speed: base_speed * 12.0,
            base_strength,
            override_crawling: false,
            blood: SimpleHealth::new(4.0),
            body,
            broken: Vec::new(),
            killed: None,
            cached: Default::default()
        };

        this.update_cache();

        this
    }

    pub fn body(&self) -> &HumanBody
    {
        &self.body
    }

    pub fn is_dead(&self) -> bool
    {
        self.speed().is_none() && self.strength().is_none()
    }

    pub fn take_killed(&mut self) -> bool
    {
        if let Some(killed) = self.killed.as_mut()
        {
            if *killed
            {
                *killed = false;
                return true;
            }
        }

        false
    }

    pub fn speed(&self) -> Option<f32>
    {
        self.cached.speed
    }

    pub fn strength(&self) -> Option<f32>
    {
        self.cached.strength
    }

    pub fn stamina(&self) -> Option<f32>
    {
        self.cached.stamina
    }

    pub fn max_stamina(&self) -> Option<f32>
    {
        self.cached.max_stamina
    }

    pub fn vision(&self) -> Option<f32>
    {
        self.cached.vision
    }

    pub fn vision_angle(&self) -> Option<f32>
    {
        self.vision().map(|x| (x * 0.5).min(1.0) * f32::consts::PI)
    }

    pub fn is_crawling(&self) -> bool
    {
        self.cached.is_crawling
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        self.base_speed = speed;

        self.update_cache();
    }

    pub fn for_accessed_parts(&mut self, mut f: impl FnMut(ChangedPart))
    {
        {
            let f = &mut f;
            mem::take(&mut self.broken).into_iter().for_each(|broken|
            {
                f(ChangedPart::whole(broken));
            });
        }

        AnatomyId::iter().for_each(|id|
        {
            let f = &mut f;

            match id
            {
                AnatomyId::Part(id) =>
                {
                    if let Some(x) = self.body.get_part_mut::<AccessedGetter>(id)
                    {
                        x(&mut |kind| f(ChangedPart::Part(id, Some(kind))));
                    }
                },
                AnatomyId::Organ(id) =>
                {
                    if self.body.get_organ_mut::<AccessedGetter>(id).unwrap_or(false)
                    {
                        f(ChangedPart::Organ(id));
                    }
                }
            }
        });
    }

    pub fn get_health(&self, id: ChangedPart) -> Option<f32>
    {
        let body = &self.body;
        match id
        {
            ChangedPart::Part(x, kind) =>
            {
                if let Some(kind) = kind
                {
                    let health = match kind
                    {
                        ChangedKind::Bone => body.get_part::<BoneHealthGetter>(x).copied(),
                        ChangedKind::Muscle => body.get_part::<MuscleHealthGetter>(x).copied().flatten(),
                        ChangedKind::Skin => body.get_part::<SkinHealthGetter>(x).copied().flatten()
                    };

                    health.map(|x| x.fraction())
                } else
                {
                    body.get_part::<AverageHealthGetter>(x)
                }
            },
            ChangedPart::Organ(x) => body.get_organ::<AverageHealthGetter>(x)
        }
    }

    fn damage_random_part(
        &mut self,
        mut damage: Damage
    ) -> Option<Damage>
    {
        if DebugConfig::is_enabled(DebugTool::PrintDamage)
        {
            eprintln!("(rng state {:?}) start damage {damage:?}", damage.rng);
        }

        let no_pierce = PierceType::empty;

        let mut ids: Vec<(AnatomyId, _)> = match damage.direction.height
        {
            DamageHeight::Top =>
            {
                match damage.direction.side
                {
                    Side2d::Back => vec![
                        (HumanPartId::Spine.into(), no_pierce()),
                        (HumanPartId::Head.into(), PierceType::head_back())
                    ],
                    Side2d::Front => vec![
                        (HumanPartId::Spine.into(), no_pierce()),
                        (HumanPartId::Head.into(), no_pierce()),
                        (OrganId::Eye(Side1d::Left).into(), PierceType::always(HumanPartId::Head.into())),
                        (OrganId::Eye(Side1d::Right).into(), PierceType::always(HumanPartId::Head.into()))
                    ],
                    Side2d::Left | Side2d::Right => vec![
                        (HumanPartId::Spine.into(), no_pierce()),
                        (HumanPartId::Head.into(), no_pierce())
                    ]
                }
            },
            DamageHeight::Middle =>
            {
                match damage.direction.side
                {
                    Side2d::Back => vec![
                        (HumanPartId::Spine.into(), PierceType::always(HumanPartId::Torso.into())),
                        (HumanPartId::Torso.into(), no_pierce()),
                        (HumanPartId::Arm(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Forearm(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Hand(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Arm(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Forearm(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Hand(Side1d::Right).into(), no_pierce())
                    ],
                    Side2d::Front => vec![
                        (HumanPartId::Torso.into(), PierceType::torso_front()),
                        (HumanPartId::Arm(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Forearm(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Hand(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Arm(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Forearm(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Hand(Side1d::Right).into(), no_pierce())
                    ],
                    Side2d::Left => vec![
                        (HumanPartId::Spine.into(), PierceType::middle_pierce(Side1d::Left)),
                        (HumanPartId::Torso.into(), PierceType::middle_pierce(Side1d::Left)),
                        (HumanPartId::Arm(Side1d::Left).into(), PierceType::arm_pierce(Side1d::Left)),
                        (HumanPartId::Forearm(Side1d::Left).into(), PierceType::arm_pierce(Side1d::Left)),
                        (HumanPartId::Hand(Side1d::Left).into(), PierceType::arm_pierce(Side1d::Left))
                    ],
                    Side2d::Right => vec![
                        (HumanPartId::Spine.into(), PierceType::middle_pierce(Side1d::Right)),
                        (HumanPartId::Torso.into(), PierceType::middle_pierce(Side1d::Right)),
                        (HumanPartId::Arm(Side1d::Right).into(), PierceType::arm_pierce(Side1d::Right)),
                        (HumanPartId::Forearm(Side1d::Right).into(), PierceType::arm_pierce(Side1d::Right)),
                        (HumanPartId::Hand(Side1d::Right).into(), PierceType::arm_pierce(Side1d::Right))
                    ]
                }
            },
            DamageHeight::Bottom =>
            {
                match damage.direction.side
                {
                    Side2d::Back | Side2d::Front => vec![
                        (HumanPartId::Pelvis.into(), no_pierce()),
                        (HumanPartId::Thigh(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Calf(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Foot(Side1d::Left).into(), no_pierce()),
                        (HumanPartId::Thigh(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Calf(Side1d::Right).into(), no_pierce()),
                        (HumanPartId::Foot(Side1d::Right).into(), no_pierce())
                    ],
                    Side2d::Left => vec![
                        (HumanPartId::Pelvis.into(), PierceType::pelvis(Side1d::Left)),
                        (HumanPartId::Thigh(Side1d::Left).into(), PierceType::leg_pierce(Side1d::Left)),
                        (HumanPartId::Calf(Side1d::Left).into(), PierceType::leg_pierce(Side1d::Left)),
                        (HumanPartId::Foot(Side1d::Left).into(), PierceType::leg_pierce(Side1d::Left))
                    ],
                    Side2d::Right => vec![
                        (HumanPartId::Pelvis.into(), PierceType::pelvis(Side1d::Right)),
                        (HumanPartId::Thigh(Side1d::Right).into(), PierceType::leg_pierce(Side1d::Right)),
                        (HumanPartId::Calf(Side1d::Right).into(), PierceType::leg_pierce(Side1d::Right)),
                        (HumanPartId::Foot(Side1d::Right).into(), PierceType::leg_pierce(Side1d::Right))
                    ]
                }
            }
        };

        ids.retain(|(id, pierce)|
        {
            self.body.get::<()>(*id).is_some() || pierce.any_exists(self)
        });

        let ids: &Vec<_> = &ids;

        let picked = WeightedPicker::pick_from(
            damage.rng.next_f64(),
            ids,
            |(id, pierce)|
            {
                self.body.get::<SizeGetter>(*id).copied().unwrap_or_else(|| pierce.combined_scale(self))
            }
        );

        let pierce = picked.and_then(|(picked, on_pierce)|
        {
            let picked_damage = self.body.get_mut::<DamagerGetter>(*picked).map(|x| x(damage.clone()));

            self.body.detach_broken(|id| { self.broken.push(id); });

            if let Some(damage) = picked_damage
            {
                damage.and_then(|pierce|
                {
                    (on_pierce.action)(self, pierce)
                })
            } else
            {
                (on_pierce.action)(self, damage)
            }
        });

        self.update_cache();

        pierce
    }

    fn speed_scale(&self) -> Speeds
    {
        let brain = some_or_return!(self.brain());

        let motor = brain.as_ref().map(|hemisphere|
        {
            Speeds{
                arms: hemisphere.frontal.motor.arms.fraction().powi(3),
                legs: hemisphere.frontal.motor.legs.fraction().powi(3)
            }
        });

        self.body.spine.as_ref().map(|spine|
        {
            let arms = spine.torso.as_ref().map(|torso|
            {
                torso.arms.as_ref().map(|arm|
                {
                    arm.as_ref().map(|x| x.arm_speed()).unwrap_or(0.0)
                })
            }).unwrap_or_else(|| Halves::repeat(0.0));

            let legs = spine.pelvis.as_ref().map(|pelvis|
            {
                pelvis.legs.as_ref().map(|leg|
                {
                    leg.as_ref().map(|x| x.leg_speed()).unwrap_or(0.0)
                })
            }).unwrap_or_else(|| Halves::repeat(0.0));

            arms.zip(legs).zip(motor.flip()).map(|((arms, legs), motor)|
            {
                Speeds{
                    legs: legs * motor.legs,
                    arms: arms * motor.arms
                }
            }).combine(|a, b| Speeds{legs: a.legs + b.legs, arms: a.arms + b.arms})
        }).unwrap_or_default()
    }

    fn brain(&self) -> Option<&Brain>
    {
        self.body.head.as_ref()?.contents.brain.as_ref()
    }

    fn updated_speed(&mut self) -> (bool, Option<f32>)
    {
        let Speeds{arms, legs} = self.speed_scale();

        let crawl_threshold = arms * 0.9; // prefer walking
        let crawling = self.override_crawling || (legs < crawl_threshold);

        let speed_scale = if !crawling
        {
            legs
        } else
        {
            arms
        };

        let speed = if speed_scale == 0.0
        {
            None
        } else
        {
            Some(self.base_speed * speed_scale)
        };

        (crawling, speed)
    }

    fn override_crawling(&mut self, state: bool)
    {
        self.override_crawling = state;
        self.update_cache();
    }

    fn updated_strength(&mut self) -> Option<f32>
    {
        let fraction = self.speed_scale().arms * 2.5;

        no_zero(self.base_strength * fraction)
    }

    fn lung(&self, side: Side1d) -> Option<&Lung>
    {
        let spine = self.body.spine.as_ref()?;
        let torso = spine.torso.as_ref()?;

        torso.torso.contents.lungs.as_ref()[side].as_ref()
    }

    fn updated_stamina(&mut self) -> Option<f32>
    {
        let base = 0.2;

        let brain = some_or_return!(self.brain());

        let amount = brain.as_ref().map_sides(|side, hemisphere|
        {
            let lung = some_or_value!(self.lung(side.opposite()), 0.0);
            lung.health.fraction() * hemisphere.frontal.motor.body.fraction().powi(3)
        }).combine(|a, b| a + b) / 2.0;

        let torso_muscle = self.body.spine.as_ref().and_then(|spine|
        {
            spine.torso.as_ref()
        }).and_then(|torso| torso.torso.muscle.map(|x| x.fraction()))
            .unwrap_or(0.0);

        no_zero(base * amount * torso_muscle)
    }

    fn updated_max_stamina(&mut self) -> Option<f32>
    {
        let base = 1.0;

        let amount = Halves{left: Side1d::Left, right: Side1d::Right}.map(|side|
        {
            some_or_value!(self.lung(side), 0.0).health.fraction()
        }).combine(|a, b| a + b) / 2.0;

        no_zero(base * amount)
    }

    fn updated_vision(&mut self) -> Option<f32>
    {
        let base = TILE_SIZE * 10.0;

        let brain = some_or_return!(self.brain());

        let vision = brain.as_ref().map(|hemisphere|
        {
            hemisphere.occipital.0.fraction().powi(3)
        }).flip().zip(self.body.head.as_ref()?.contents.eyes.as_ref()).map(|(fraction, eye)|
        {
            eye.as_ref().map(|x| x.average_health()).unwrap_or(0.0) * fraction
        }).combine(|a, b| a.max(b));

        no_zero(base * vision)
    }

    fn update_cache(&mut self)
    {
        (self.cached.is_crawling, self.cached.speed) = self.updated_speed();
        self.cached.strength = self.updated_strength();
        self.cached.stamina = self.updated_stamina();
        self.cached.max_stamina = self.updated_max_stamina();
        self.cached.vision = self.updated_vision();

        if self.is_dead() && self.killed.is_none()
        {
            self.killed = Some(true);
        }
    }
}

impl Damageable for HumanAnatomy
{
    fn damage(&mut self, mut damage: Damage) -> Option<Damage>
    {
        if self.is_crawling()
        {
            damage = damage * 2.0;
        }

        self.damage_random_part(damage)
    }

    fn is_full(&self) -> bool
    {
        self.body.is_full()
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        self.body.heal(amount)
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
