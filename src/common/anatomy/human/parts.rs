use std::fmt::{self, Display};

use serde::{Serialize, Deserialize};

use strum::{EnumCount, FromRepr, IntoStaticStr};

use crate::common::{
    from_upper_camel,
    DamageType,
    Side1d,
    Side2d
};

use super::super::{
    health_iter_mut_helper as iter_helper,
    Health,
    Halves,
    BodyPart,
    Organ,
    HealthField,
    HealthIterate,
    HealReceiver,
    DamageReceiver,
    PartFieldGetter,
    RefOrganFieldGet,
    RefHumanPartFieldGet,
    RefMutOrganFieldGet,
    RefMutHumanPartFieldGet
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotorCortex
{
    pub arms: HealthField,
    pub body: HealthField,
    pub legs: HealthField
}

impl MotorCortex
{
    pub fn new(base: f32) -> Self
    {
        Self{
            arms: Health::new(0.1, base).into(),
            body: Health::new(0.1, base).into(),
            legs: Health::new(0.1, base).into()
        }
    }
}

impl HealthIterate for MotorCortex
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        [&self.arms, &self.body, &self.legs].into_iter()
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        let order = vec![&mut self.arms, &mut self.body, &mut self.legs];

        let order = match side
        {
            Side2d::Left | Side2d::Right =>
            {
                if fastrand::bool()
                {
                    order.into_iter().rev().collect()
                } else
                {
                    order
                }
            },
            Side2d::Front | Side2d::Back =>
            {
                order
            }
        };

        order.into_iter()
    }
}

impl HealReceiver for MotorCortex {}
impl DamageReceiver for MotorCortex {}

impl Organ for MotorCortex
{
    fn size(&self) -> &f64
    {
        &0.05
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontalLobe
{
    pub motor: MotorCortex
}

impl FrontalLobe
{
    pub fn new(base: f32) -> Self
    {
        Self{motor: MotorCortex::new(base)}
    }
}

impl HealthIterate for FrontalLobe
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.motor.health_iter()
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        self.motor.health_sided_iter_mut(side)
    }
}

impl HealReceiver for FrontalLobe {}
impl DamageReceiver for FrontalLobe {}

impl Organ for FrontalLobe
{
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
pub struct ParietalLobe(pub HealthField);

impl ParietalLobe
{
    pub fn new(base: f32) -> Self
    {
        Self(Health::new(0.1, base).into())
    }
}

impl HealthIterate for ParietalLobe
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        [&self.0].into_iter()
    }

    fn health_sided_iter_mut(&mut self, _side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        [&mut self.0].into_iter()
    }
}

impl HealReceiver for ParietalLobe {}
impl DamageReceiver for ParietalLobe {}

impl Organ for ParietalLobe
{
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
pub struct TemporalLobe(pub HealthField);

impl TemporalLobe
{
    pub fn new(base: f32) -> Self
    {
        Self(Health::new(0.1, base).into())
    }
}

impl HealthIterate for TemporalLobe
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        [&self.0].into_iter()
    }

    fn health_sided_iter_mut(&mut self, _side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        [&mut self.0].into_iter()
    }
}

impl HealReceiver for TemporalLobe {}
impl DamageReceiver for TemporalLobe {}

impl Organ for TemporalLobe
{
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
pub struct OccipitalLobe(pub HealthField);

impl OccipitalLobe
{
    pub fn new(base: f32) -> Self
    {
        Self(Health::new(0.1, base).into())
    }
}

impl HealthIterate for OccipitalLobe
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        [&self.0].into_iter()
    }

    fn health_sided_iter_mut(&mut self, _side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        [&mut self.0].into_iter()
    }
}

impl HealReceiver for OccipitalLobe {}
impl DamageReceiver for OccipitalLobe {}

impl Organ for OccipitalLobe
{
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
    pub frontal: FrontalLobe,
    pub parietal: ParietalLobe,
    pub temporal: TemporalLobe,
    pub occipital: OccipitalLobe
}

impl Hemisphere
{
    pub fn new(base: f32) -> Self
    {
        Self{
            frontal: FrontalLobe::new(base),
            parietal: ParietalLobe::new(base),
            temporal: TemporalLobe::new(base),
            occipital: OccipitalLobe::new(base)
        }
    }
}

impl HealthIterate for Hemisphere
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.frontal.health_iter()
            .chain(self.parietal.health_iter())
            .chain(self.temporal.health_iter())
            .chain(self.occipital.health_iter())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        let order = match side
        {
            Side2d::Left | Side2d::Right =>
            {
                let mut order = [
                    iter_helper(side, &mut self.frontal),
                    iter_helper(side, &mut self.parietal),
                    iter_helper(side, &mut self.occipital),
                    iter_helper(side, &mut self.temporal)
                ];

                fastrand::shuffle(&mut order);

                order
            },
            Side2d::Front | Side2d::Back =>
            {
                let (middle, last) = if fastrand::bool()
                {
                    (iter_helper(side, &mut self.parietal), iter_helper(side, &mut self.temporal))
                } else
                {
                    (iter_helper(side, &mut self.temporal), iter_helper(side, &mut self.parietal))
                };

                if let Side2d::Front = side
                {
                    [iter_helper(side, &mut self.frontal), middle, iter_helper(side, &mut self.occipital), last]
                } else
                {
                    [iter_helper(side, &mut self.occipital), middle, iter_helper(side, &mut self.frontal), last]
                }
            }
        };

        order.into_iter().flatten()
    }
}

impl HealReceiver for Hemisphere {}
impl DamageReceiver for Hemisphere {}

impl Organ for Hemisphere
{
    fn size(&self) -> &f64
    {
        &0.1
    }
}

pub type Brain = Halves<Hemisphere>;

impl Brain
{
    pub fn new(base: f32) -> Self
    {
        Self::repeat(Hemisphere::new(base))
    }
}

impl HealthIterate for Brain
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        [&self.left, &self.right].into_iter().flat_map(|x| x.health_iter())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        let order = match side
        {
            Side2d::Left =>
            {
                [&mut self.left, &mut self.right]
            },
            Side2d::Right =>
            {
                [&mut self.right, &mut self.left]
            },
            Side2d::Front | Side2d::Back =>
            {
                if fastrand::bool()
                {
                    [&mut self.right, &mut self.left]
                } else
                {
                    [&mut self.left, &mut self.right]
                }
            }
        };

        order.into_iter().flat_map(move |x| x.health_sided_iter_mut(side))
    }
}

impl HealReceiver for Brain {}
impl DamageReceiver for Brain {}

impl Organ for Brain
{
    fn size(&self) -> &f64
    {
        &0.2
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eye(pub HealthField);

impl Eye
{
    pub fn new(base: f32) -> Self
    {
        Self(Health::new(0.5, base).into())
    }
}

impl HealthIterate for Eye
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        [&self.0].into_iter()
    }

    fn health_sided_iter_mut(&mut self, _side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        [&mut self.0].into_iter()
    }
}

impl HealReceiver for Eye {}
impl DamageReceiver for Eye {}

impl Organ for Eye
{
    fn size(&self) -> &f64
    {
        &0.1
    }

    fn consume_accessed(&mut self) -> bool
    {
        self.0.consume_accessed()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lung(pub HealthField);

impl Lung
{
    pub fn new(base: f32) -> Self
    {
        Self(Health::new(0.1, base).into())
    }
}

impl HealthIterate for Lung
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        [&self.0].into_iter()
    }

    fn health_sided_iter_mut(&mut self, _side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        [&mut self.0].into_iter()
    }
}

impl HealReceiver for Lung {}
impl DamageReceiver for Lung {}

impl Organ for Lung
{
    fn size(&self) -> &f64
    {
        &0.3
    }

    fn consume_accessed(&mut self) -> bool
    {
        self.0.consume_accessed()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpinalCord
{
    pub cervical: HealthField,
    pub lumbar: HealthField
}

impl SpinalCord
{
    pub fn new(base: f32) -> Self
    {
        Self{
            cervical: Health::new(0.2, base).into(),
            lumbar: Health::new(0.1, base * 0.5).into()
        }
    }
}

impl HealthIterate for SpinalCord
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        [&self.cervical, &self.lumbar].into_iter()
    }

    fn health_sided_iter_mut(&mut self, _side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        [&mut self.cervical, &mut self.lumbar].into_iter()
    }
}

impl HealReceiver for SpinalCord {}
impl DamageReceiver for SpinalCord {}

impl Organ for SpinalCord
{
    fn size(&self) -> &f64
    {
        &0.3
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
pub enum SpinalCordId
{
    Cervical,
    Lumbar
}

impl Display for SpinalCordId
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "{}", match self
        {
            Self::Cervical => "cervical",
            Self::Lumbar => "lumbar"
        })
    }
}

impl SpinalCordId
{
    pub fn iter() -> impl Iterator<Item=Self>
    {
        [
            Self::Cervical,
            Self::Lumbar
        ].into_iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrganId
{
    Eye(Side1d),
    Brain(Option<Side1d>, Option<BrainId>),
    SpinalCord(SpinalCordId),
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
            Self::SpinalCord(part) => return write!(f, "spinal cord ({part})"),
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
            .chain(SpinalCordId::iter().map(Self::SpinalCord))
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadOrgans
{
    pub eyes: Halves<Option<Eye>>,
    pub brain: Option<Brain>
}

impl HealthIterate for HeadOrgans
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.brain.as_ref().map(|x| x.health_iter()).into_iter().flatten()
            .chain(self.eyes.health_iter())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        self.brain.as_mut().map(move |x| x.health_sided_iter_mut(side)).into_iter().flatten()
            .chain(self.eyes.health_sided_iter_mut(side))
    }
}

impl HealReceiver for HeadOrgans {}

impl DamageReceiver for HeadOrgans
{
    fn damage_normal(
        &mut self,
        side: Side2d,
        damage: DamageType
    ) -> Option<DamageType>
    {
        self.brain.as_mut().map(move |x| x.health_sided_iter_mut(side)).into_iter().flatten()
            .try_fold(damage, |acc, x|
            {
                x.damage_pierce(acc, 1.0)
            })
    }
}

impl Organ for HeadOrgans
{
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

impl HealthIterate for TorsoOrgans
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.lungs.health_iter()
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        self.lungs.health_sided_iter_mut(side)
    }
}

impl HealReceiver for TorsoOrgans {}
impl DamageReceiver for TorsoOrgans {}

impl Organ for TorsoOrgans
{
    fn size(&self) -> &f64
    {
        unimplemented!()
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
    pub lower: HumanPart,
    pub leaf: Option<HumanPart>
}

impl HealthIterate for LowerLimb
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.lower.health_iter()
            .chain(self.leaf.as_ref().map(|x| x.health_iter()).into_iter().flatten())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        self.lower.health_sided_iter_mut(side)
            .chain(self.leaf.as_mut().map(|x| x.health_sided_iter_mut(side)).into_iter().flatten())
    }
}

impl HealReceiver for LowerLimb {}

impl LowerLimb
{
    pub fn detach_broken(&mut self, on_break: impl FnOnce())
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
            x.speed_multiply(leaf, self.lower.muscle.fraction())
        }).unwrap_or_default();

        self.lower.speed_multiply(lower, None) + leaf_speed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limb
{
    pub upper: HumanPart,
    pub lower: Option<LowerLimb>
}

impl HealthIterate for Limb
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.upper.health_iter()
            .chain(self.lower.as_ref().map(|x| x.health_iter()).into_iter().flatten())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        self.upper.health_sided_iter_mut(side)
            .chain(self.lower.as_mut().map(|x| x.health_sided_iter_mut(side)).into_iter().flatten())
    }
}

impl HealReceiver for Limb {}

impl Limb
{
    pub fn detach_broken<OnBreak: FnMut(AnatomyId)>(
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

    pub fn arm_speed(&self) -> f32
    {
        self.speed_with(0.2, 0.1, 0.05)
    }

    pub fn leg_speed(&self) -> f32
    {
        self.speed_with(0.4, 0.12, 0.07)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pelvis
{
    pub pelvis: HumanPart,
    pub legs: Halves<Option<Limb>>
}

impl HealthIterate for Pelvis
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.pelvis.health_iter()
            .chain(self.legs.health_iter())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        self.pelvis.health_sided_iter_mut(side)
            .chain(self.legs.health_sided_iter_mut(side))
    }
}

impl HealReceiver for Pelvis {}

impl Pelvis
{
    pub fn detach_broken(&mut self, on_break: &mut impl FnMut(AnatomyId))
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
    pub spine: BodyPart<SpinalCord>,
    pub torso: Option<HumanPart<TorsoOrgans>>,
    pub arms: Halves<Option<Limb>>,
    pub pelvis: Option<Pelvis>
}

impl HealthIterate for Spine
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.spine.health_iter()
            .chain(self.arms.health_iter())
            .chain(self.torso.as_ref().map(|x| x.health_iter()).into_iter().flatten())
            .chain(self.pelvis.as_ref().map(|x| x.health_iter()).into_iter().flatten())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        self.spine.health_sided_iter_mut(side)
            .chain(self.arms.health_sided_iter_mut(side))
            .chain(self.torso.as_mut().map(|x| x.health_sided_iter_mut(side)).into_iter().flatten())
            .chain(self.pelvis.as_mut().map(|x| x.health_sided_iter_mut(side)).into_iter().flatten())
    }
}

impl HealReceiver for Spine {}

impl Spine
{
    pub fn detach_broken(&mut self, on_break: &mut impl FnMut(AnatomyId))
    {
        remove_broken!(self.torso, || on_break(AnatomyId::Part(HumanPartId::Torso)));
        remove_broken!(self.pelvis, || on_break(AnatomyId::Part(HumanPartId::Pelvis)), pelvis);

        if let Some(torso) = self.torso.as_mut()
        {
            torso.contents.lungs.as_mut().map_sides(|side, lung|
            {
                remove_broken!(lung, || on_break(AnatomyId::Organ(OrganId::Lung(side))));
            });
        }

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

impl HealthIterate for HumanBody
{
    fn health_iter(&self) -> impl Iterator<Item=&HealthField>
    {
        self.head.as_ref().map(|x| x.health_iter()).into_iter().flatten()
            .chain(self.spine.as_ref().map(|x| x.health_iter()).into_iter().flatten())
    }

    fn health_sided_iter_mut(&mut self, side: Side2d) -> impl Iterator<Item=&mut HealthField>
    {
        self.head.as_mut().map(|x| x.health_sided_iter_mut(side)).into_iter().flatten()
            .chain(self.spine.as_mut().map(|x| x.health_sided_iter_mut(side)).into_iter().flatten())
    }
}

impl HealReceiver for HumanBody {}

impl HumanBody
{
    pub fn detach_broken(&mut self, mut on_break: impl FnMut(AnatomyId))
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
                    Some(F::get(self.head.$option_fn()?.contents.eyes[side].$option_fn()?))
                },
                OrganId::Lung(side) =>
                {
                    Some(F::get(self.spine.$option_fn()?
                        .torso.$option_fn()?
                        .contents.lungs[side].$option_fn()?))
                },
                OrganId::SpinalCord(part) =>
                {
                    let spine = $($b)+ self.spine.$option_fn()?.spine.contents;

                    let part = match part
                    {
                        SpinalCordId::Cervical => $($b)+ spine.cervical,
                        SpinalCordId::Lumbar => $($b)+ spine.lumbar
                    };

                    Some(F::get(part))
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

            let value = match id
            {
                HumanPartId::Head => unreachable!(),
                HumanPartId::Spine => unreachable!(),
                HumanPartId::Torso => F::get(torso?),
                HumanPartId::Pelvis => F::get($($b)+ pelvis?.pelvis),
                HumanPartId::Thigh(side) => F::get($($b)+ pelvis?.legs[side].$option_fn()?.upper),
                HumanPartId::Calf(side) => F::get($($b)+ pelvis?.legs[side].$option_fn()?.lower.$option_fn()?.lower),
                HumanPartId::Foot(side) => F::get(pelvis?.legs[side].$option_fn()?.lower.$option_fn()?.leaf.$option_fn()?),
                HumanPartId::Arm(side) => F::get($($b)+ spine.arms[side].$option_fn()?.upper),
                HumanPartId::Forearm(side) => F::get($($b)+ spine.arms[side].$option_fn()?.lower.$option_fn()?.lower),
                HumanPartId::Hand(side) => F::get(spine.arms[side].$option_fn()?.lower.$option_fn()?.leaf.$option_fn()?)
            };

            Some(value)
        }
    }
}

impl HumanBody
{
    impl_get!{RefHumanPartFieldGet, RefOrganFieldGet, get, get_part, get_organ, as_ref, &}
    impl_get!{RefMutHumanPartFieldGet, RefMutOrganFieldGet, get_mut, get_part_mut, get_organ_mut, as_mut, &mut}
}
