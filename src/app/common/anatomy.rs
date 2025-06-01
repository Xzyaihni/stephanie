use std::{
    f32,
    iter,
    convert,
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

pub trait PartFieldGetter
{
    type V<'a>;

    fn get<C: Organ>(value: &HumanPart<C>) -> &Self::V<'_>;
    fn get_mut<C: Organ>(value: &mut HumanPart<C>) -> &mut Self::V<'_>;
    fn run<C: Organ>(value: &mut HumanPart<C>) -> Self::V<'_>;
}

macro_rules! simple_field_getter
{
    ($name:ident, $t:ty, $f:ident) =>
    {
        pub struct $name;

        impl PartFieldGetter for $name
        {
            type V<'a> = $t;

            fn get<T: Organ>(value: &HumanPart<T>) -> &Self::V<'_> { &value.$f }
            fn get_mut<T: Organ>(value: &mut HumanPart<T>) -> &mut Self::V<'_> { &mut value.$f }
            fn run<T: Organ>(_value: &mut HumanPart<T>) -> Self::V<'_> { unreachable!() }
        }
    }
}

simple_field_getter!{BoneHealthGetter, Health, bone}
simple_field_getter!{MuscleHealthGetter, Option<Health>, muscle}
simple_field_getter!{SkinHealthGetter, Option<Health>, skin}
simple_field_getter!{SizeGetter, f64, size}

pub struct AverageHealthGetter;
impl PartFieldGetter for AverageHealthGetter
{
    type V<'a> = f32;

    fn get<T: Organ>(_value: &HumanPart<T>) -> &Self::V<'_> { unreachable!() }
    fn get_mut<T: Organ>(_value: &mut HumanPart<T>) -> &mut Self::V<'_> { unreachable!() }
    fn run<T: Organ>(_value: &mut HumanPart<T>) -> Self::V<'_> { unreachable!() }
}

impl PartFieldGetter for ()
{
    type V<'a> = ();

    fn get<T: Organ>(_value: &HumanPart<T>) -> &Self::V<'_> { &() }
    fn get_mut<T: Organ>(_value: &mut HumanPart<T>) -> &mut Self::V<'_> { unreachable!() }
    fn run<T: Organ>(_value: &mut HumanPart<T>) -> Self::V<'_> { }
}

struct DamagerGetter;
impl PartFieldGetter for DamagerGetter
{
    type V<'a> = Box<dyn FnOnce(Damage) -> Option<Damage> + 'a>;

    fn get<T: Organ>(_value: &HumanPart<T>) -> &Self::V<'_> { unreachable!() }
    fn get_mut<T: Organ>(_value: &mut HumanPart<T>) -> &mut Self::V<'_> { unreachable!() }
    fn run<T: Organ>(value: &mut HumanPart<T>) -> Self::V<'_>
    {
        Box::new(|damage|
        {
            value.damage(damage)
        })
    }
}

struct AccessedGetter;
impl PartFieldGetter for AccessedGetter
{
    type V<'a> = Box<dyn FnOnce(&mut dyn FnMut(ChangedKind)) + 'a>;

    fn get<T: Organ>(_value: &HumanPart<T>) -> &Self::V<'_> { unreachable!() }
    fn get_mut<T: Organ>(_value: &mut HumanPart<T>) -> &mut Self::V<'_> { unreachable!() }
    fn run<T: Organ>(value: &mut HumanPart<T>) -> Self::V<'_>
    {
        Box::new(|f| { value.consume_accessed(f) })
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
    simple_getter!(stamina);
    simple_getter!(max_stamina);
    simple_getter!(vision);
    simple_getter!(vision_angle);

    pub fn get_human<F: PartFieldGetter>(
        &self,
        id: AnatomyId
    ) -> Option<Option<&F::V<'_>>>
    {
        #[allow(irrefutable_let_patterns)]
        if let Self::Human(x) = self
        {
            Some(x.body.get::<F>(id))
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
}

pub trait DamageReceiver
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>;
}

pub enum ChangedKind
{
    Bone,
    Muscle,
    Skin,
    Organ(OrganId)
}

pub struct ChangedPart
{
    pub id: HumanPartId,
    pub kind: ChangedKind
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SimpleHealth
{
    max: f32,
    current: f32
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
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Health
{
    max_block: f32,
    health: SimpleHealth
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

    pub fn current(&self) -> f32
    {
        self.health.current()
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
    fn clear(&mut self);
    fn consume_accessed<F: FnMut(OrganId)>(&mut self, f: F);
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
    fn clear(&mut self) {}
    fn consume_accessed<F: FnMut(OrganId)>(&mut self, _f: F) {}
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
                    if self.bone.is_zero()
                    {
                        self.contents.clear();
                    }

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

        self.contents.consume_accessed(|x| f(ChangedKind::Organ(x)));
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

impl MotorCortex
{
    fn consume_accessed(&mut self, mut f: impl FnMut(MotorId))
    {
        if self.arms.consume_accessed()
        {
            f(MotorId::Arms)
        }

        if self.body.consume_accessed()
        {
            f(MotorId::Body)
        }

        if self.legs.consume_accessed()
        {
            f(MotorId::Legs)
        }
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

impl FrontalLobe
{
    fn consume_accessed(&mut self, mut f: impl FnMut(FrontalId))
    {
        self.motor.consume_accessed(|id| f(FrontalId::Motor(id)));
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

#[derive(Debug, Clone, Copy, FromRepr, EnumCount, Serialize, Deserialize)]
pub enum LobeId
{
    Frontal,
    Parietal,
    Temporal,
    Occipital
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hemisphere
{
    frontal: FrontalLobe,
    parietal: ChangeTracking<Health>,
    temporal: ChangeTracking<Health>,
    occipital: ChangeTracking<Health>
}

impl Default for Hemisphere
{
    fn default() -> Self
    {
        Self{
            frontal: FrontalLobe::default(),
            parietal: Health::new(4.0, 50.0).into(),
            temporal: Health::new(4.0, 50.0).into(),
            occipital: Health::new(4.0, 50.0).into()
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
            LobeId::Parietal => self.parietal.damage_pierce(damage),
            LobeId::Temporal => self.temporal.damage_pierce(damage),
            LobeId::Occipital => self.occipital.damage_pierce(damage)
        }
    }

    fn consume_accessed(&mut self, mut f: impl FnMut(BrainId))
    {
        self.frontal.consume_accessed(|id| f(BrainId::Frontal(id)));

        if self.parietal.consume_accessed()
        {
            f(BrainId::Parietal)
        }

        if self.temporal.consume_accessed()
        {
            f(BrainId::Temporal)
        }

        if self.occipital.consume_accessed()
        {
            f(BrainId::Occipital)
        }
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

pub type Brain = Halves<Hemisphere>;

impl Default for Brain
{
    fn default() -> Self
    {
        Self{left: Hemisphere::default(), right: Hemisphere::default()}
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MotorId
{
    Arms,
    Body,
    Legs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrontalId
{
    Motor(MotorId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrainId
{
    Frontal(FrontalId),
    Parietal,
    Temporal,
    Occipital
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrganId
{
    Eye(Side1d),
    Brain(Side1d, BrainId),
    Lung(Side1d)
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

        let mut previous_uppercase = true;
        let name: String = s.chars().flat_map(|c|
        {
            let is_uppercase = c.is_uppercase();
            let c = c.to_lowercase();

            if is_uppercase && !previous_uppercase
            {
                return iter::once(' ').chain(c).collect::<Vec<_>>();
            }

            previous_uppercase = is_uppercase;

            c.collect::<Vec<_>>()
        }).collect();

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

    pub fn side_name(&self) -> String
    {
        self.side().map(|x|
        {
            let s: &str = x.into();

            format!("{} ", s.to_lowercase())
        }).unwrap_or_default()
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
            Self::Hand(_) => "hand", // lmao i cant rly pick any of the bones
            Self::Foot(_) => "foot" // same with this one
        }
    }
}

pub type HumanPart<Contents=()> = BodyPart<Contents>;

#[derive(Debug, Clone)]
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
    fn clear(&mut self)
    {
        self.brain = None;
    }

    fn consume_accessed<F: FnMut(OrganId)>(&mut self, mut f: F)
    {
        if let Some(brain) = self.brain.as_mut()
        {
            brain.as_mut().map_sides(|side, hemisphere|
            {
                hemisphere.consume_accessed(|id| f(OrganId::Brain(side, id)));
            });
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorsoOrgans
{
    pub lungs: Halves<Option<Lung>>
}

impl Organ for TorsoOrgans
{
    fn clear(&mut self)
    {
        self.lungs.as_mut().map(|x| { *x = None; });
    }

    fn consume_accessed<F: FnMut(OrganId)>(&mut self, mut f: F)
    {
        self.lungs.as_mut().map_sides(|side, lung|
        {
            if let Some(lung) = lung.as_mut()
            {
                if lung.health.consume_accessed() { f(OrganId::Lung(side)); }
            }
        });
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanBodySided
{
    pub eye: Option<HumanPart>,
    pub upper_leg: Option<HumanPart>,
    pub lower_leg: Option<HumanPart>,
    pub upper_arm: Option<HumanPart>,
    pub lower_arm: Option<HumanPart>,
    pub hand: Option<HumanPart>,
    pub foot: Option<HumanPart>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanBody
{
    pub sided: Halves<HumanBodySided>,
    pub head: HumanPart<HeadOrgans>,
    pub torso: HumanPart<TorsoOrgans>,
    pub pelvis: HumanPart,
    pub spine: HumanPart
}

macro_rules! impl_get
{
    ($fn_name:ident, $part_fn_name:ident, $organ_fn_name:ident, $option_fn:ident, $rt:ty, $($b:tt)+) =>
    {
        pub fn $fn_name<F: PartFieldGetter>(
            $($b)+ self,
            id: AnatomyId
        ) -> Option<$rt>
        {
            match id
            {
                AnatomyId::Organ(id) => self.$organ_fn_name::<F>(id),
                AnatomyId::Part(id) => self.$part_fn_name::<F>(id)
            }
        }

        pub fn $organ_fn_name<F: PartFieldGetter>(
            $($b)+ self,
            id: OrganId
        ) -> Option<$rt>
        {
            todo!()
        }

        pub fn $part_fn_name<F: PartFieldGetter>(
            $($b)+ self,
            id: HumanPartId
        ) -> Option<$rt>
        {
            match id
            {
                HumanPartId::Head => Some(F::$fn_name($($b)+ self.head)),
                HumanPartId::Torso => Some(F::$fn_name($($b)+ self.torso)),
                HumanPartId::Pelvis => Some(F::$fn_name($($b)+ self.pelvis)),
                HumanPartId::Spine => Some(F::$fn_name($($b)+ self.spine)),
                HumanPartId::Thigh(side) => self.sided[side].upper_leg.$option_fn().map(|x| F::$fn_name(x)),
                HumanPartId::Calf(side) => self.sided[side].lower_leg.$option_fn().map(|x| F::$fn_name(x)),
                HumanPartId::Foot(side) => self.sided[side].foot.$option_fn().map(|x| F::$fn_name(x)),
                HumanPartId::Arm(side) => self.sided[side].upper_arm.$option_fn().map(|x| F::$fn_name(x)),
                HumanPartId::Forearm(side) => self.sided[side].lower_arm.$option_fn().map(|x| F::$fn_name(x)),
                HumanPartId::Hand(side) => self.sided[side].hand.$option_fn().map(|x| F::$fn_name(x))
            }
        }
    }
}

impl HumanBody
{
    impl_get!{get, get_part, get_organ, as_ref, &F::V<'_>, &}
    impl_get!{get_mut, get_part_mut, get_organ_mut, as_mut, &mut F::V<'_>, &mut}
    impl_get!{run, run_part, run_organ, as_mut, F::V<'_>, &mut}
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

    fn head_back() -> Self
    {
        let possible = vec![OrganId::Eye(Side1d::Left).into(), OrganId::Eye(Side1d::Right).into()];

        Self::possible_pierce(possible, 1, convert::identity)
    }

    fn torso_front() -> Self
    {
        Self::possible_pierce(vec![HumanPartId::Spine.into()], 2, convert::identity)
    }

    fn possible_pierce<F>(possible: Vec<AnatomyId>, misses: usize, f: F) -> Self
    where
        F: Fn(Option<Damage>) -> Option<Damage> + 'static
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
                    return f(None);
                }

                let miss_check = damage.rng.next_usize_between(0..possible_actions.len() + misses);
                if miss_check >= possible_actions.len()
                {
                    return f(None);
                }

                let target = damage.rng.choice(possible_actions);

                f(this.body.run::<DamagerGetter>(target).unwrap()(damage))
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

        Self::possible_pierce(possible, 1, convert::identity)
    }

    fn arm_pierce(side: Side1d) -> PierceType
    {
        Self{
            possible: vec![HumanPartId::Spine.into(), HumanPartId::Torso.into()],
            action: Rc::new(move |this: &mut HumanAnatomy, mut damage|
            {
                let target = if damage.rng.next_bool()
                {
                    HumanPartId::Spine
                } else
                {
                    HumanPartId::Torso
                };

                let pierce = some_or_value!(
                    this.body.run::<DamagerGetter>(target.into()).unwrap()(damage),
                    None
                );

                (Self::middle_pierce(side).action)(this, pierce)
            })
        }
    }

    fn leg_pierce(side: Side1d) -> PierceType
    {
        let opposite = side.opposite();

        let possible = vec![
            HumanPartId::Thigh(opposite).into(),
            HumanPartId::Calf(opposite).into(),
            HumanPartId::Foot(opposite).into()
        ];

        Self::possible_pierce(possible, 0, convert::identity)
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
        let base_strength = info.base_strength;
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

        let make_side = |side_name|
        {
            let with_name = |name|
            {
                format!("{side_name} {name}")
            };

            let upper_leg = Some(new_part(DebugName::new(with_name("upper leg")), 4000.0, 0.6));
            let lower_leg = Some(new_part(DebugName::new(with_name("lower leg")), 3500.0, 0.44));
            let foot = Some(new_part(DebugName::new(with_name("foot")), 2000.0, 0.17));

            let upper_arm = Some(new_part(DebugName::new(with_name("upper arm")), 2500.0, 0.2));
            let lower_arm = Some(new_part(DebugName::new(with_name("lower arm")), 2000.0, 0.17));
            let hand = Some(new_part(DebugName::new(with_name("hand")), 2000.0, 0.07));

            let eye = Some(HumanPart::new_full(
                DebugName::new(with_name("eye")),
                Health::new(50.0, 100.0),
                None,
                None,
                0.1,
                ()
            ));

            HumanBodySided{
                eye,
                upper_leg,
                lower_leg,
                upper_arm,
                lower_arm,
                hand,
                foot
            }
        };

        let sided = Halves{left: make_side("left"), right: make_side("right")};

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

        let body = HumanBody{
            sided,
            head,
            torso,
            pelvis,
            spine
        };

        let mut this = Self{
            base_speed: base_speed * 12.0,
            base_strength,
            override_crawling: false,
            blood: SimpleHealth::new(4.0),
            body,
            cached: Default::default()
        };

        this.update_cache();

        this
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
        HumanPartId::iter().for_each(|id|
        {
            let f = &mut f;
            if let Some(x) = self.body.run::<AccessedGetter>(id.into())
            {
                x(&mut |kind| f(ChangedPart{id, kind}));
            }
        });
    }

    fn damage_random_part(
        &mut self,
        mut damage: Damage
    ) -> Option<Damage>
    {
        if DebugConfig::is_enabled(DebugTool::PrintDamage)
        {
            eprintln!("start damage {damage:?}");
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
                        (OrganId::Eye(Side1d::Left).into(), no_pierce()),
                        (OrganId::Eye(Side1d::Right).into(), no_pierce())
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
                        (HumanPartId::Spine.into(), no_pierce()),
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
                        (HumanPartId::Pelvis.into(), no_pierce()),
                        (HumanPartId::Thigh(Side1d::Left).into(), PierceType::leg_pierce(Side1d::Left)),
                        (HumanPartId::Calf(Side1d::Left).into(), PierceType::leg_pierce(Side1d::Left)),
                        (HumanPartId::Foot(Side1d::Left).into(), PierceType::leg_pierce(Side1d::Left))
                    ],
                    Side2d::Right => vec![
                        (HumanPartId::Pelvis.into(), no_pierce()),
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
            let picked_damage = self.body.run::<DamagerGetter>(*picked).map(|x| x(damage.clone()));
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

    fn speed_multiply(part: &HumanPart, base: f32) -> f32
    {
        let muscle_health = part.muscle.as_ref().map(|x| x.fraction()).unwrap_or(0.0);
        let health_mult = (part.bone.fraction() * 0.9 + 0.1) * muscle_health;

        base * health_mult
    }

    fn leg_speed(body: &HumanBodySided) -> f32
    {
        body.upper_leg.as_ref().map(|x| Self::speed_multiply(x, 0.4)).unwrap_or_default()
            + body.lower_leg.as_ref().map(|x| Self::speed_multiply(x, 0.12)).unwrap_or_default()
            + body.foot.as_ref().map(|x| Self::speed_multiply(x, 0.07)).unwrap_or_default()
    }

    fn arm_speed(body: &HumanBodySided) -> f32
    {
        body.upper_arm.as_ref().map(|x| Self::speed_multiply(x, 0.2)).unwrap_or_default()
            + body.lower_arm.as_ref().map(|x| Self::speed_multiply(x, 0.1)).unwrap_or_default()
            + body.hand.as_ref().map(|x| Self::speed_multiply(x, 0.05)).unwrap_or_default()
    }

    fn speed_scale(body: &HumanBody, motor: Halves<Speeds>) -> Speeds
    {
        body.sided.as_ref().zip(motor.flip()).map(|(body, motor)|
        {
            Speeds{
                legs: Self::leg_speed(body) * motor.legs,
                arms: Self::arm_speed(body) * motor.arms
            }
        }).combine(|a, b| Speeds{legs: a.legs + b.legs, arms: a.arms + b.arms})
    }

    fn brain(&self) -> Option<&Brain>
    {
        self.body.head.contents.brain.as_ref()
    }

    fn updated_speed(&mut self) -> (bool, Option<f32>)
    {
        let brain = some_or_value!(self.brain(), (false, None));

        let speeds = brain.as_ref().map(|hemisphere|
        {
            Speeds{
                arms: hemisphere.frontal.motor.arms.fraction().powi(3),
                legs: hemisphere.frontal.motor.legs.fraction().powi(3)
            }
        });

        let Speeds{arms, legs} = Self::speed_scale(&self.body, speeds);

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
        Some(self.base_strength)
    }

    fn lung(&self, side: Side1d) -> Option<&Lung>
    {
        let contents = &self.body.torso.contents.lungs;

        if side.is_left()
        {
            contents.left.as_ref()
        } else
        {
            contents.right.as_ref()
        }
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

        Some(base * amount * self.body.torso.muscle.map(|x| x.fraction()).unwrap_or(0.0))
    }

    fn updated_max_stamina(&mut self) -> Option<f32>
    {
        let base = 1.0;

        let amount = Halves{left: Side1d::Left, right: Side1d::Right}.map(|side|
        {
            some_or_value!(self.lung(side), 0.0).health.fraction()
        }).combine(|a, b| a + b) / 2.0;

        Some(base * amount)
    }

    fn updated_vision(&mut self) -> Option<f32>
    {
        let base = TILE_SIZE * 10.0;

        let brain = some_or_return!(self.brain());

        let vision = brain.as_ref().map(|hemisphere|
        {
            hemisphere.occipital.fraction().powi(3)
        }).flip().zip(self.body.sided.as_ref()).map(|(fraction, body)|
        {
            body.eye.as_ref().map(|x| x.bone.fraction()).unwrap_or(0.0) * fraction
        }).combine(|a, b| a.max(b));

        Some(base * vision)
    }

    fn update_cache(&mut self)
    {
        (self.cached.is_crawling, self.cached.speed) = self.updated_speed();
        self.cached.strength = self.updated_strength();
        self.cached.stamina = self.updated_stamina();
        self.cached.max_stamina = self.updated_max_stamina();
        self.cached.vision = self.updated_vision();
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
}
