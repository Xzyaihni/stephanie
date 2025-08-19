use std::fmt::{self, Display};

use serde::{Serialize, Deserialize};

use strum::{EnumCount, FromRepr, IntoStaticStr};

use crate::common::{
    from_upper_camel,
    Side1d,
    Side2d,
    DamageType,
    SeededRandom
};

use super::super::{
    heal_iterative,
    Health,
    Halves,
    BodyPart,
    Organ,
    ChangeTracking,
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
    pub arms: ChangeTracking<Health>,
    pub body: ChangeTracking<Health>,
    pub legs: ChangeTracking<Health>
}

impl MotorCortex
{
    pub fn new(base: f32) -> Self
    {
        Self{
            arms: Health::new(base * 0.1, base).into(),
            body: Health::new(base * 0.1, base).into(),
            legs: Health::new(base * 0.1, base).into()
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
    pub motor: MotorCortex
}

impl FrontalLobe
{
    pub fn new(base: f32) -> Self
    {
        Self{motor: MotorCortex::new(base)}
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
pub struct ParietalLobe(pub ChangeTracking<Health>);

impl ParietalLobe
{
    pub fn new(base: f32) -> Self
    {
        Self(Health::new(base * 0.1, base).into())
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
pub struct TemporalLobe(pub ChangeTracking<Health>);

impl TemporalLobe
{
    pub fn new(base: f32) -> Self
    {
        Self(Health::new(base * 0.1, base).into())
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
pub struct OccipitalLobe(pub ChangeTracking<Health>);

impl OccipitalLobe
{
    pub fn new(base: f32) -> Self
    {
        Self(Health::new(base * 0.1, base).into())
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

impl Brain
{
    pub fn new(base: f32) -> Self
    {
        Self::repeat(Hemisphere::new(base))
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
    pub health: ChangeTracking<Health>
}

impl Eye
{
    pub fn new(base: f32) -> Self
    {
        Self{health: Health::new(base * 0.5, base).into()}
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
    pub health: ChangeTracking<Health>
}

impl Lung
{
    pub fn new(base: f32) -> Self
    {
        Self{health: Health::new(base * 0.05, base).into()}
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
    pub lower: HumanPart,
    pub leaf: Option<HumanPart>
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
            let muscle = self.lower.muscle.map(|x| x.fraction()).unwrap_or(0.0);
            x.speed_multiply(leaf, Some(muscle))
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
    pub spine: HumanPart,
    pub torso: Option<HumanPart<TorsoOrgans>>,
    pub arms: Halves<Option<Limb>>,
    pub pelvis: Option<Pelvis>
}

impl HealReceiver for Spine
{
    fn is_full(&self) -> bool
    {
        self.spine.is_full()
            && self.torso.as_ref().map(|x| x.is_full()).unwrap_or(true)
            && self.arms.as_ref().map(|x| x.as_ref().map(|x| x.is_full()).unwrap_or(true)).combine(|a, b| a && b)
            && self.pelvis.as_ref().map(|x| x.is_full()).unwrap_or(true)
    }

    fn heal(&mut self, amount: f32) -> Option<f32>
    {
        heal_iterative(amount, [
            &mut self.spine,
            self.torso.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.arms.left.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.arms.right.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ()),
            self.pelvis.as_mut().map(|x| -> &mut dyn HealReceiver { x }).unwrap_or(&mut ())
        ])
    }
}

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
                    self.head.$option_fn()?.contents.eyes[side].$option_fn().map(|x| F::get(x))
                },
                OrganId::Lung(side) =>
                {
                    self.spine.$option_fn()
                        .and_then(|x| x.torso.$option_fn())
                        .and_then(|x| x.contents.lungs[side].$option_fn())
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
