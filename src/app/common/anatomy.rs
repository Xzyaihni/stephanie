use std::{
    iter,
    ops::{Index, IndexMut, ControlFlow}
};

use serde::{Serialize, Deserialize};

use strum::{EnumCount, FromRepr};

use nalgebra::Vector3;

use crate::common::{
    some_or_value,
    SeededRandom,
    WeightedPicker,
    Damage,
    DamageDirection,
    DamageHeight,
    DamageType,
    Side1d,
    Side2d,
    Side3d,
    Damageable,
    world::TILE_SIZE
};


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

    pub fn sees(&self, this_position: &Vector3<f32>, other_position: &Vector3<f32>) -> bool
    {
        let distance = this_position.metric_distance(other_position);

        self.vision().unwrap_or(0.0) >= distance
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        match self
        {
            Self::Human(x) => x.set_speed(speed)
        }
    }
}

impl Damageable for Anatomy
{
    fn damage(&mut self, damage: Damage) -> Option<DamageType>
    {
        match self
        {
            Self::Human(x) => x.damage(damage)
        }
    }
}

trait DamageReceiver
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PartId
{
    This,
    Next{id: Side3d, next: Box<PartId>}
}

impl PartId
{
    pub fn replace_tail(&self, id: Side3d) -> Self
    {
        match self
        {
            Self::This => Self::Next{id, next: Box::new(Self::This)},
            Self::Next{id: this_id, next} =>
            {
                Self::Next{id: *this_id, next: Box::new(next.replace_tail(id))}
            }
        }
    }

    pub fn with_parent(self, parent: Side3d) -> Self
    {
        Self::Next{id: parent, next: Box::new(self)}
    }

    pub fn is_child_of(&self, other: &Self) -> bool
    {
        match (self, other)
        {
            (_, Self::This) => true,
            (Self::This, Self::Next{..}) => false,
            (Self::Next{id, next}, Self::Next{id: other_id, next: other_next}) =>
            {
                if *id != *other_id
                {
                    false
                } else
                {
                    next.is_child_of(other_next)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoneChildren<T>([Option<T>; Side3d::COUNT]);

impl<T: Clone> From<Vec<(Side3d, T)>> for BoneChildren<T>
{
    fn from(values: Vec<(Side3d, T)>) -> Self
    {
        let mut output = Self::empty();

        for (key, value) in values
        {
            let spot = output.get_mut(key);

            if spot.is_some()
            {
                panic!("duplicate definition of {key:?}");
            }

            *spot = Some(value);
        }

        output
    }
}

impl<T> BoneChildren<T>
{
    pub fn empty() -> Self
    where
        T: Clone
    {
        let values = iter::repeat(None)
            .take(Side3d::COUNT)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or_else(|_| unreachable!());

        Self(values)
    }

    pub fn is_empty(&self) -> bool
    {
        self.0.iter().all(|x| x.is_none())
    }

    pub fn clear(&mut self)
    {
        self.0.iter_mut().for_each(|x| *x = None);
    }

    pub fn get(&self, index: Side3d) -> &Option<T>
    {
        self.0.get(index as usize).unwrap()
    }

    pub fn get_mut(&mut self, index: Side3d) -> &mut Option<T>
    {
        self.0.get_mut(index as usize).unwrap()
    }

    pub fn iter(&self) -> impl Iterator<Item=(Side3d, &T)>
    {
        self.0.iter().enumerate().filter_map(|(index, value)|
        {
            value.as_ref().map(|value| (Side3d::from_repr(index).unwrap(), value))
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item=(Side3d, &mut T)>
    {
        self.0.iter_mut().enumerate().filter_map(|(index, value)|
        {
            value.as_mut().map(|value| (Side3d::from_repr(index).unwrap(), value))
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bone<Data>
{
    data: Data,
    children: Box<BoneChildren<BodyPart<Bone<Data>>>>
}

impl<Data: Clone> Bone<Data>
{
    pub fn new(data: Data, children: BoneChildren<BodyPart<Bone<Data>>>) -> Self
    {
        Self{data, children: Box::new(children)}
    }

    pub fn leaf(data: Data) -> Self
    {
        Self::new(data, BoneChildren::empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleHealth
{
    max: f32,
    current: f32
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health
{
    max_block: f32,
    health: SimpleHealth
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
pub struct BodyPart<Data>
{
    this: Health,
    skin: Option<Health>,
    muscle: Option<Health>,
    size: f64,
    part: Data
}

impl<Data> BodyPart<Data>
{
    pub fn new(info: BodyPartInfo, this: f32, size: f64, part: Data) -> Self
    {
        Self::new_full(
            Health::new(this * 0.05, this),
            Some(Health::new(info.skin_toughness * 5.0, info.skin_toughness * 100.0)),
            Some(Health::new(info.muscle_toughness * 20.0, info.muscle_toughness * 500.0)),
            size,
            part
        )
    }

    pub fn new_full(
        this: Health,
        skin: Option<Health>,
        muscle: Option<Health>,
        size: f64,
        part: Data
    ) -> Self
    {
        Self{this, skin, muscle, size, part}
    }
}

impl<Data> BodyPart<Bone<Data>>
{
    pub fn get<'a>(&'a self, index: &PartId) -> Option<&'a Self>
    {
        match index
        {
            PartId::This => Some(self),
            PartId::Next{id, next} =>
            {
                self.part.children.get(*id).as_ref()?.get(next)
            }
        }
    }

    pub fn get_mut<'a>(&'a mut self, index: &PartId) -> Option<&'a mut Self>
    {
        match index
        {
            PartId::This => Some(self),
            PartId::Next{id, next} =>
            {
                self.part.children.get_mut(*id).as_mut()?.get_mut(next)
            }
        }
    }

    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    where
        Data: DamageReceiver
    {
        // huh
        if let Some(pierce) = self.skin.as_mut().map(|x| x.damage_pierce(damage))
            .unwrap_or(Some(damage))
        {
            if let Some(pierce) = self.muscle.as_mut().map(|x| x.damage_pierce(damage))
                .unwrap_or(Some(pierce))
            {
                if let Some(pierce) = self.this.damage_pierce(pierce)
                {
                    if self.this.is_zero()
                    {
                        self.part.children.clear();
                    }

                    return self.part.data.damage(rng, side, pierce);
                }
            }
        }

        None
    }

    pub fn enumerate(&self, mut f: impl FnMut(&PartId))
    {
        self.enumerate_inner(PartId::This, &mut f)
    }

    pub fn enumerate_with(&self, start: PartId, mut f: impl FnMut(&PartId))
    {
        self.enumerate_inner(start, &mut f)
    }

    fn enumerate_inner(&self, part_id: PartId, f: &mut impl FnMut(&PartId))
    {
        f(&part_id);

        self.part.children.iter().for_each(|(id, child)|
        {
            child.enumerate_inner(part_id.replace_tail(id), f)
        });
    }

    pub fn for_each(&self, mut f: impl FnMut(&Self))
    {
        self.for_each_inner(&mut f);
    }

    fn for_each_inner(&self, f: &mut impl FnMut(&Self))
    {
        f(self);

        self.part.children.iter().for_each(|(_, child)| child.for_each_inner(f));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Halves<T>
{
    left: T,
    right: T
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

impl<T> Halves<T>
{
    pub fn map<U, F: FnMut(T) -> U>(self, mut f: F) -> Halves<U>
    {
        Halves{
            left: f(self.left),
            right: f(self.right)
        }
    }

    pub fn map_ref<U, F: FnMut(&T) -> U>(&self, mut f: F) -> Halves<U>
    {
        Halves{
            left: f(&self.left),
            right: f(&self.right)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotorCortex
{
    arms: Health,
    legs: Health
}

impl Default for MotorCortex
{
    fn default() -> Self
    {
        Self{
            arms: Health::new(4.0, 50.0),
            legs: Health::new(4.0, 50.0)
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
        let mut order = vec![&mut self.arms, &mut self.legs];

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
    parietal: Health,
    temporal: Health,
    occipital: Health
}

impl Default for Hemisphere
{
    fn default() -> Self
    {
        Self{
            frontal: FrontalLobe::default(),
            parietal: Health::new(4.0, 50.0),
            temporal: Health::new(4.0, 50.0),
            occipital: Health::new(4.0, 50.0)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lung
{
    health: Health,
    side: Side1d
}

impl Lung
{
    fn left() -> Self
    {
        Self::new(Side1d::Left)
    }

    fn right() -> Self
    {
        Self::new(Side1d::Right)
    }

    fn new(side: Side1d) -> Self
    {
        Self{health: Health::new(3.0, 20.0), side}
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HumanOrgan
{
    Brain(Brain),
    Lung(Lung)
}

impl DamageReceiver for HumanOrgan
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        match self
        {
            Self::Brain(brain) =>
            {
                let hemispheres = [&mut brain.left, &mut brain.right];

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
            },
            Self::Lung(lung) =>
            {
                lung.damage(rng, side, damage)
            }
        }
    }
}

macro_rules! impl_contents
{
    ($self:ident) =>
    {
        match $self
        {
            Self::Skull{contents} => Some(contents),
            Self::Ribcage{contents} => Some(contents),
            _ => None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HumanBoneSingle
{
    Skull{contents: Vec<HumanOrgan>},
    Ribcage{contents: Vec<HumanOrgan>},
    // the eye bone lol
    Eye,
    Spine,
    Pelvis,
    Femur,
    Tibia,
    Humerus,
    Radius,
    Hand,
    Foot
}

impl HumanBoneSingle
{
    pub fn contents(&self) -> Option<&[HumanOrgan]>
    {
        impl_contents!(self)
    }

    pub fn contents_mut(&mut self) -> Option<&mut [HumanOrgan]>
    {
        impl_contents!(self)
    }
}

impl DamageReceiver for HumanBoneSingle
{
    fn damage(
        &mut self,
        rng: &mut SeededRandom,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        if let Some(contents) = self.contents_mut()
        {
            contents.iter_mut().for_each(|organ|
            {
                organ.damage(rng, side, damage);
            });
        }

        None
    }
}

pub type HumanBone = Bone<HumanBoneSingle>;
pub type HumanPart = BodyPart<HumanBone>;

#[derive(Debug, Clone)]
struct LimbSpeed(f32);

impl LimbSpeed
{
    fn new(speed: f32) -> Self
    {
        Self(speed)
    }

    fn resolve(self, health_mult: f32, motor: Option<f32>, children: f32) -> f32
    {
        let motor = motor.unwrap_or(1.0);

        children + self.0 * health_mult * motor
    }
}

#[derive(Debug, Clone)]
struct Speeds
{
    arms: f32,
    legs: f32
}

#[derive(Debug, Clone)]
struct SpeedsState
{
    conseq_leg: u32,
    conseq_arm: u32,
    side: Option<Side1d>,
    halves: Halves<Speeds>
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
pub struct HumanAnatomy
{
    base_speed: f32,
    base_strength: f32,
    override_crawling: bool,
    blood: SimpleHealth,
    body: HumanPart,
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

        let new_part = |health, size, other|
        {
            HumanPart::new(
                part.clone(),
                bone_toughness * health,
                size,
                other
            )
        };

        // max hp is amount of newtons i found on the interner needed to break a bone
        // like half of them i just made up
        let leg = new_part(
            4000.0,
            0.6,
            HumanBone::new(
                HumanBoneSingle::Femur,
                vec![
                    (Side3d::Bottom, new_part(
                        3500.0,
                        0.44,
                        HumanBone::new(
                            HumanBoneSingle::Tibia,
                            vec![
                                (Side3d::Bottom, new_part(
                                    5000.0,
                                    0.17,
                                    HumanBone::leaf(HumanBoneSingle::Foot)
                                ))
                            ].into()
                        )
                    ))
                ].into()
            )
        );

        let arm = new_part(
            2500.0,
            0.2,
            HumanBone::new(
                HumanBoneSingle::Humerus,
                vec![
                    (Side3d::Bottom, new_part(
                        2000.0,
                        0.17,
                        HumanBone::new(
                            HumanBoneSingle::Radius,
                            vec![
                                (Side3d::Bottom, new_part(
                                    4000.0,
                                    0.07,
                                    HumanBone::leaf(HumanBoneSingle::Hand)
                                ))
                            ].into()
                        )
                    ))
                ].into()
            )
        );

        let eye = HumanPart::new_full(
            Health::new(50.0, 100.0),
            None,
            None,
            0.01,
            HumanBone::leaf(HumanBoneSingle::Eye)
        );

        // the spine is very complex sizing wise so im just gonna pick a low-ish number
        let body = new_part(
            3400.0,
            0.25,
            HumanBone::new(
                HumanBoneSingle::Spine,
                vec![
                    (Side3d::Top, new_part(
                        5000.0,
                        0.39,
                        HumanBone::new(
                            HumanBoneSingle::Skull{contents: vec![
                                HumanOrgan::Brain(Brain::default())
                            ]},
                            vec![
                                (Side3d::Left, eye.clone()),
                                (Side3d::Right, eye)
                            ].into()
                        )
                    )),
                    (Side3d::Bottom, new_part(
                        6000.0,
                        0.37,
                        HumanBone::new(
                            HumanBoneSingle::Pelvis,
                            vec![
                                (Side3d::Left, leg.clone()),
                                (Side3d::Right, leg)
                            ].into()
                        )
                    )),
                    (Side3d::Front, new_part(
                        3300.0,
                        0.82,
                        HumanBone::new(
                            HumanBoneSingle::Ribcage{contents: vec![
                                HumanOrgan::Lung(Lung::left()),
                                HumanOrgan::Lung(Lung::right())
                            ]},
                            vec![
                                (Side3d::Left, arm.clone()),
                                (Side3d::Right, arm)
                            ].into()
                        )
                    ))
                ].into()
            )
        );

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

    pub fn is_crawling(&self) -> bool
    {
        self.cached.is_crawling
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        self.base_speed = speed;

        self.update_cache();
    }

    fn select_random_part(
        &mut self,
        rng: &mut SeededRandom,
        direction: DamageDirection
    ) -> Option<&mut HumanPart>
    {
        let child_side = match direction.height
        {
            DamageHeight::Top => Side3d::Top,
            DamageHeight::Bottom => Side3d::Bottom,
            DamageHeight::Middle => Side3d::Front
        };

        let mut occluded_parts = match direction.side
        {
            Side2d::Front | Side2d::Back => Vec::new(),
            Side2d::Left | Side2d::Right =>
            {
                match child_side
                {
                    Side3d::Top => Vec::new(),
                    Side3d::Front | Side3d::Bottom =>
                    {
                        vec![
                            PartId::This
                                .with_parent(direction.side.opposite().into())
                                .with_parent(child_side)
                        ]
                    },
                    _ => unreachable!()
                }
            }
        };

        #[allow(clippy::single_match)]
        match child_side
        {
            Side3d::Top =>
            {
                match direction.side
                {
                    Side2d::Front => (),
                    _ =>
                    {
                        let left_eye = PartId::This
                            .with_parent(Side3d::Left)
                            .with_parent(child_side);

                        let right_eye = PartId::This
                            .with_parent(Side3d::Right)
                            .with_parent(child_side);

                        occluded_parts.extend([left_eye, right_eye]);
                    }
                }
            },
            _ => ()
        }

        let mut ids = Vec::new();

        if let Some(child) = self.body.part.children.get(child_side)
        {
            let start = PartId::Next{id: child_side, next: Box::new(PartId::This)};
            child.enumerate_with(start, |id|
            {
                let skip = occluded_parts.iter().any(|skip_part|
                {
                    id.is_child_of(skip_part)
                });

                if !skip
                {
                    ids.push(id.clone());
                }
            });
        }

        // u can hit the spine at any height
        ids.push(PartId::This);

        let ids: &Vec<_> = &ids;

        let picked = WeightedPicker::pick_from(
            rng.next_f64(),
            ids,
            |id| self.body.get(id).expect("must be inbounds").size
        );

        // borrow checker silliness
        if let Some(picked) = picked
        {
            Some(self.body.get_mut(picked).expect("must be inbounds"))
        } else
        {
            #[allow(clippy::manual_find_map)]
            occluded_parts.iter().rev()
                .find(|id| self.body.get_mut(id).is_some())
                .map(|id| self.body.get_mut(id).expect("must be inbounds"))
        }
    }

    fn leg_speed(
        conseq: &mut u32,
        bone: &HumanBone
    ) -> f32
    {
        match bone.data
        {
            HumanBoneSingle::Femur =>
            {
                *conseq = 1;
                0.4
            },
            HumanBoneSingle::Tibia =>
            {
                if *conseq == 1
                {
                    *conseq = 2;
                    0.12
                } else
                {
                    *conseq = 0;
                    0.05
                }
            },
            HumanBoneSingle::Foot =>
            {
                let value = if bone.children.is_empty() && *conseq == 2
                {
                    0.07
                } else
                {
                    0.01
                };

                *conseq = 0;

                value
            },
            _ =>
            {
                *conseq = 0;
                0.0
            }
        }
    }

    fn arm_speed(
        conseq: &mut u32,
        bone: &HumanBone
    ) -> f32
    {
        match bone.data
        {
            HumanBoneSingle::Humerus =>
            {
                *conseq = 1;
                0.2
            },
            HumanBoneSingle::Radius =>
            {
                if *conseq == 1
                {
                    *conseq = 2;
                    0.1
                } else
                {
                    *conseq = 0;
                    0.05
                }
            },
            HumanBoneSingle::Hand =>
            {
                let value = if bone.children.is_empty() && *conseq == 2
                {
                    0.05
                } else
                {
                    0.01
                };

                *conseq = 0;

                value
            },
            _ =>
            {
                *conseq = 0;

                0.0
            }
        }
    }

    fn speed_scale(
        state: &mut SpeedsState,
        part: &HumanPart
    ) -> Speeds
    {
        let muscle_health = part.muscle.as_ref().map(|x| x.fraction()).unwrap_or(0.0);
        let health_mult = (part.this.fraction() * 0.9 + 0.1) * muscle_health;

        let motor = state.side.as_ref().map(|side| &state.halves[*side]);

        let bone = &part.part;

        let leg_speed = Self::leg_speed(&mut state.conseq_leg, bone);
        let arm_speed = Self::arm_speed(&mut state.conseq_arm, bone);

        let children_speed: Speeds = bone.children.iter().map(|(side, child)|
        {
            let mut state = state.clone();

            if let Ok(side) = side.try_into()
            {
                // :/
                let side: Side1d = side;

                state.side = Some(side.opposite());
            }

            Self::speed_scale(&mut state, child)
        }).reduce(|acc, x| Speeds{arms: acc.arms + x.arms, legs: acc.legs + x.legs})
            .unwrap_or(Speeds{arms: 0.0, legs: 0.0});

        Speeds{
            arms: LimbSpeed::new(arm_speed)
                .resolve(health_mult, motor.map(|x| x.arms), children_speed.arms),
            legs: LimbSpeed::new(leg_speed)
                .resolve(health_mult, motor.map(|x| x.legs), children_speed.legs)
        }
    }

    fn brain_search(bone: &HumanBone) -> Option<&Brain>
    {
        if let Some(organs) = bone.data.contents()
        {
            let found = organs.iter().find_map(|organ|
            {
                match organ
                {
                    HumanOrgan::Brain(brain) => Some(brain),
                    _ => None
                }
            });

            if found.is_some()
            {
                return found;
            }
        }

        bone.children.iter().find_map(|(_, part)| Self::brain_search(&part.part))
    }

    fn brain(&self) -> Option<&Brain>
    {
        Self::brain_search(&self.body.part)
    }

    fn updated_speed(&mut self) -> (bool, Option<f32>)
    {
        let brain = some_or_value!(self.brain(), (false, None));

        let mut state = SpeedsState{
            conseq_leg: 0,
            conseq_arm: 0,
            side: None,
            halves: brain.map_ref(|hemisphere|
            {
                Speeds{
                    arms: hemisphere.frontal.motor.arms.fraction(),
                    legs: hemisphere.frontal.motor.legs.fraction()
                }
            })
        };

        let Speeds{arms, legs} = Self::speed_scale(&mut state, &self.body);

        let legs = legs * state.halves.left.legs;
        let arms = arms * state.halves.left.arms;

        let crawl_speed = arms;
        let crawling = self.override_crawling || (legs < crawl_speed);

        let speed_scale = if !crawling
        {
            legs
        } else
        {
            crawl_speed
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

    fn updated_stamina(&mut self) -> Option<f32>
    {
        Some(0.5)
    }

    fn updated_max_stamina(&mut self) -> Option<f32>
    {
        Some(10.0)
    }

    fn updated_vision(&mut self) -> Option<f32>
    {
        Some(TILE_SIZE * 8.0)
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
    fn damage(&mut self, mut damage: Damage) -> Option<DamageType>
    {
        if self.is_crawling()
        {
            damage = damage.scale(2.0);
        }

        if let Some(part) = self.select_random_part(&mut damage.rng, damage.direction)
        {
            let pierce = part.damage(&mut damage.rng, damage.direction.side, damage.data);

            self.update_cache();

            pierce
        } else
        {
            None
        }
    }
}
