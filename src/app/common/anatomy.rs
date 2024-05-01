use std::{
    iter,
    ops::{Index, IndexMut}
};

use serde::{Serialize, Deserialize};

use strum::EnumCount;
use strum_macros::{EnumCount, FromRepr};

use crate::common::{
    SeededRandom,
    WeightedPicker,
    Damage,
    DamageDirection,
    DamageHeight,
    Side2d,
    Damageable
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Anatomy
{
    Human(HumanAnatomy)
}

impl Anatomy
{
    pub fn speed(&self) -> Option<f32>
    {
        match self
        {
            Self::Human(x) => x.speed()
        }
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
    fn damage(&mut self, damage: Damage)
    {
        match self
        {
            Self::Human(x) => x.damage(damage)
        }
    }
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
pub struct Health
{
    max: f32,
    current: f32
}

impl From<f32> for Health
{
    fn from(value: f32) -> Self
    {
        Self::new(value)
    }
}

impl Health
{
    pub fn new(max: f32) -> Self
    {
        Self{max, current: max}
    }

    pub fn fraction(&self) -> f32
    {
        self.current / self.max
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyPart<Data>
{
    bone: Health,
    skin: Health,
    muscle: Health,
    size: f64,
    part: Data
}

impl<Data> BodyPart<Data>
{
    pub fn new(bone: impl Into<Health>, size: f64, part: Data) -> Self
    {
        Self::new_full(bone, 100.0, 500.0, size, part)
    }

    pub fn new_full(
        bone: impl Into<Health>,
        skin: impl Into<Health>,
        muscle: impl Into<Health>,
        size: f64,
        part: Data
    ) -> Self
    {
        Self{bone: bone.into(), skin: skin.into(), muscle: muscle.into(), size, part}
    }
}

impl<Data> BodyPart<Bone<Data>>
{
    pub fn get<'a>(&self, index: &'a PartId) -> &'_ Self
    {
        match index
        {
            PartId::This => self,
            PartId::Next{id, next} =>
            {
                self.part.children.get(*id).as_ref().expect("out of bounds").get(next)
            }
        }
    }

    pub fn get_mut<'a>(&mut self, index: &'a PartId) -> &'_ mut Self
    {
        match index
        {
            PartId::This => self,
            PartId::Next{id, next} =>
            {
                self.part.children.get_mut(*id).as_mut().expect("out of bounds").get_mut(next)
            }
        }
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Halves<T>
{
    left: T,
    right: T
}

impl<T> Index<Side> for Halves<T>
{
    type Output = T;

    fn index(&self, side: Side) -> &Self::Output
    {
        match side
        {
            Side::Left => &self.left,
            Side::Right => &self.right
        }
    }
}

impl<T> IndexMut<Side> for Halves<T>
{
    fn index_mut(&mut self, side: Side) -> &mut Self::Output
    {
        match side
        {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Side
{
    Left,
    Right
}

impl TryFrom<Side3d> for Side
{
    type Error = ();

    fn try_from(side: Side3d) -> Result<Self, ()>
    {
        match side
        {
            Side3d::Left => Ok(Self::Left),
            Side3d::Right => Ok(Self::Right),
            _ => Err(())
        }
    }
}

impl TryFrom<Side2d> for Side
{
    type Error = ();

    fn try_from(side: Side2d) -> Result<Self, ()>
    {
        match side
        {
            Side2d::Left => Ok(Self::Left),
            Side2d::Right => Ok(Self::Right),
            _ => Err(())
        }
    }
}

impl Side
{
    pub fn opposite(self) -> Self
    {
        match self
        {
            Self::Left => Self::Right,
            Self::Right => Self::Left
        }
    }
}

impl From<Side> for Side2d
{
    fn from(side: Side) -> Self
    {
        match side
        {
            Side::Left => Self::Left,
            Side::Right => Self::Right
        }
    }
}

#[derive(Debug, Clone, Copy, EnumCount, FromRepr, Serialize, Deserialize)]
pub enum Side3d
{
    Left,
    Right,
    Top,
    Bottom,
    Front,
    Back
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eye
{
    side: Side
}

impl Eye
{
    fn left() -> Self
    {
        Self{side: Side::Left}
    }

    fn right() -> Self
    {
        Self{side: Side::Right}
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
            arms: Health::new(5.0),
            legs: Health::new(5.0)
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
            parietal: Health::new(5.0),
            temporal: Health::new(5.0),
            occipital: Health::new(5.0)
        }
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
    side: Side
}

impl Lung
{
    fn left() -> Self
    {
        Self{side: Side::Left}
    }

    fn right() -> Self
    {
        Self{side: Side::Right}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HumanOrgan
{
    Eye(Eye),
    Brain(Brain),
    Lung(Lung)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HumanBoneSingle
{
    Skull{contents: Vec<HumanOrgan>},
    Ribcage{contents: Vec<HumanOrgan>},
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
        match self
        {
            Self::Skull{contents} => Some(contents),
            Self::Ribcage{contents} => Some(contents),
            _ => None
        }
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
    side: Option<Side>,
    halves: Halves<Speeds>
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CachedProps
{
    speed: Option<f32>,
    attack: Option<f32>,
    vision: Option<f32>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanAnatomy
{
    base_speed: f32,
    body: HumanPart,
    cached: CachedProps
}

impl Default for HumanAnatomy
{
    fn default() -> Self
    {
        // max hp is amount of newtons i found on the interner needed to break a bone
        // like half of them i just made up
        let leg = HumanPart::new(
            4000.0,
            0.6,
            HumanBone::new(
                HumanBoneSingle::Femur,
                vec![
                    (Side3d::Bottom, HumanPart::new(
                        3500.0,
                        0.44,
                        HumanBone::new(
                            HumanBoneSingle::Tibia,
                            vec![
                                (Side3d::Bottom, HumanPart::new(
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

        let arm = HumanPart::new(
            2500.0,
            0.2,
            HumanBone::new(
                HumanBoneSingle::Humerus,
                vec![
                    (Side3d::Bottom, HumanPart::new(
                        2000.0,
                        0.17,
                        HumanBone::new(
                            HumanBoneSingle::Radius,
                            vec![
                                (Side3d::Bottom, HumanPart::new(
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

        // the spine is very complex sizing wise so im just gonna pick a low-ish number
        let body = HumanPart::new(
            3400.0,
            0.25,
            HumanBone::new(
                HumanBoneSingle::Spine,
                vec![
                    (Side3d::Top, HumanPart::new(
                        5000.0,
                        0.39,
                        HumanBone::leaf(HumanBoneSingle::Skull{contents: vec![
                            HumanOrgan::Brain(Brain::default()),
                            HumanOrgan::Eye(Eye::left()),
                            HumanOrgan::Eye(Eye::right())
                        ]})
                    )),
                    (Side3d::Bottom, HumanPart::new(
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
                    (Side3d::Front, HumanPart::new(
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
            base_speed: 12.0,
            body,
            cached: Default::default()
        };

        this.update();

        this
    }
}

impl HumanAnatomy
{
    pub fn speed(&self) -> Option<f32>
    {
        self.cached.speed
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        self.base_speed = speed;

        self.update();
    }

    fn select_random_part(
        &mut self,
        mut rng: SeededRandom,
        direction: DamageDirection
    ) -> Option<&mut HumanPart>
    {
        let child_side = match direction.height
        {
            DamageHeight::Top => Side3d::Top,
            DamageHeight::Bottom => Side3d::Bottom,
            DamageHeight::Middle => Side3d::Front
        };

        let mut ids = Vec::new();

        if let Some(child) = self.body.part.children.get(child_side)
        {
            let start = PartId::Next{id: child_side, next: Box::new(PartId::This)};
            child.enumerate_with(start, |id| ids.push(id.clone()));
        }

        // u can hit the spine at any height
        ids.push(PartId::This);

        let ids: &Vec<_> = &ids;

        let picked = WeightedPicker::pick_from(rng.next_f64(), ids, |id| self.body.get(id).size);

        picked.map(|picked| self.body.get_mut(picked))
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
        let health_mult = (part.bone.fraction() * 0.9 + 0.1) * part.muscle.fraction();

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
                let side: Side = side;

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

    fn updated_speed(&mut self) -> Option<f32>
    {
        let brain = self.brain()?;

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

        let speed_scale = if legs > arms
        {
            legs
        } else
        {
            arms + legs
        };

        if speed_scale == 0.0
        {
            None
        } else
        {
            Some(self.base_speed * speed_scale)
        }
    }

    fn update(&mut self)
    {
        self.cached.speed = self.updated_speed();
    }
}

impl Damageable for HumanAnatomy
{
    fn damage(&mut self, damage: Damage)
    {
        let part = self.select_random_part(damage.rng, damage.direction);

        dbg!(part, damage.data);
    }
}
