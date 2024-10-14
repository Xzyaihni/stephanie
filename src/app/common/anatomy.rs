use std::{
    convert,
    rc::Rc,
    fmt::{self, Debug, Display},
    ops::{Index, IndexMut, ControlFlow}
};

use serde::{Serialize, Deserialize};

use strum::{EnumCount, FromRepr};

use nalgebra::Vector3;

use crate::{
    debug_config::*,
    common::{
        some_or_value,
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
    fn damage(&mut self, damage: Damage) -> Option<Damage>
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

#[derive(Clone, Serialize, Deserialize)]
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
pub struct BodyPart<Data>
{
    name: DebugName,
    bone: Health,
    skin: Option<Health>,
    muscle: Option<Health>,
    size: f64,
    contents: Vec<Data>
}

impl<Data> BodyPart<Data>
{
    pub fn new(
        name: DebugName,
        info: BodyPartInfo,
        bone: f32,
        size: f64,
        contents: Vec<Data>
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
        contents: Vec<Data>
    ) -> Self
    {
        Self{name, bone, skin, muscle, size, contents}
    }
}

impl<Data> BodyPart<Data>
{
    fn damage(&mut self, damage: Damage) -> Option<Damage>
    where
        Data: DamageReceiver + Debug
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
                if let Some(pierce) = self.bone.damage_pierce(pierce)
                {
                    if self.bone.is_zero()
                    {
                        self.contents.clear();
                    }

                    if self.contents.is_empty()
                    {
                        return Some(pierce);
                    }

                    let id = rng.next_usize_between(0..self.contents.len());

                    return self.contents[id].damage(rng, side, pierce);
                }
            }
        }

        None
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
    pub fn as_ref(&self) -> Halves<&T>
    {
        Halves{
            left: &self.left,
            right: &self.right
        }
    }

    pub fn zip<U>(self, other: Halves<U>) -> Halves<(T, U)>
    {
        Halves{
            left: (self.left, other.left),
            right: (self.right, other.right)
        }
    }

    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> Halves<U>
    {
        Halves{
            left: f(self.left),
            right: f(self.right)
        }
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
        if DebugConfig::is_enabled(DebugTool::PrintDamage)
        {
            eprintln!("damaging {self:?} at {side:?} for {damage:?}");
        }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HumanPartId
{
    Head,
    Torso,
    Spine,
    Pelvis,
    Eye(Side1d),
    UpperLeg(Side1d),
    LowerLeg(Side1d),
    UpperArm(Side1d),
    LowerArm(Side1d),
    Hand(Side1d),
    Foot(Side1d)
}

pub type HumanPart = BodyPart<HumanOrgan>;

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
    pub head: HumanPart,
    pub torso: HumanPart,
    pub pelvis: HumanPart,
    pub spine: HumanPart
}

macro_rules! impl_get
{
    ($fn_name:ident, $option_fn:ident, $($b:tt)+) =>
    {
        pub fn $fn_name($($b)+ self, id: HumanPartId) -> Option<$($b)+ HumanPart>
        {
            match id
            {
                HumanPartId::Head => Some($($b)+ self.head),
                HumanPartId::Torso => Some($($b)+ self.torso),
                HumanPartId::Pelvis => Some($($b)+ self.pelvis),
                HumanPartId::Spine => Some($($b)+ self.spine),
                HumanPartId::Eye(side) => self.sided[side].eye.$option_fn(),
                HumanPartId::UpperLeg(side) => self.sided[side].upper_leg.$option_fn(),
                HumanPartId::LowerLeg(side) => self.sided[side].lower_leg.$option_fn(),
                HumanPartId::Foot(side) => self.sided[side].foot.$option_fn(),
                HumanPartId::UpperArm(side) => self.sided[side].upper_arm.$option_fn(),
                HumanPartId::LowerArm(side) => self.sided[side].lower_arm.$option_fn(),
                HumanPartId::Hand(side) => self.sided[side].hand.$option_fn()
            }
        }
    }
}

impl HumanBody
{
    impl_get!{get, as_ref, &}
    impl_get!{get_mut, as_mut, &mut}
}

struct PierceType
{
    possible: Vec<HumanPartId>,
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
        let possible = vec![HumanPartId::Eye(Side1d::Left), HumanPartId::Eye(Side1d::Right)];

        Self::possible_pierce(possible, 1, convert::identity)
    }

    fn possible_pierce<F>(possible: Vec<HumanPartId>, misses: usize, f: F) -> Self
    where
        F: Fn(Option<Damage>) -> Option<Damage> + 'static
    {
        let possible_cloned = possible.clone();

        Self{
            possible,
            action: Rc::new(move |this: &mut HumanAnatomy, mut damage|
            {
                let mut possible_actions = possible_cloned.clone();
                possible_actions.retain(|x| this.body.get(*x).is_some());

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

                f(this.body.get_mut(target).unwrap().damage(damage))
            })
        }
    }

    fn middle_pierce(side: Side1d) -> PierceType
    {
        let opposite = side.opposite();

        let possible = vec![
            HumanPartId::UpperArm(opposite),
            HumanPartId::LowerArm(opposite),
            HumanPartId::Hand(opposite)
        ];

        Self::possible_pierce(possible, 1, convert::identity)
    }

    fn arm_pierce(side: Side1d) -> PierceType
    {
        Self{
            possible: vec![HumanPartId::Spine, HumanPartId::Torso],
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
                    this.body.get_mut(target).unwrap().damage(damage),
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
            HumanPartId::UpperLeg(opposite),
            HumanPartId::LowerLeg(opposite),
            HumanPartId::Foot(opposite)
        ];

        Self::possible_pierce(possible, 0, convert::identity)
    }

    fn any_exists(&self, anatomy: &HumanAnatomy) -> bool
    {
        self.possible.iter().any(|x| anatomy.body.get(*x).is_some())
    }

    fn combined_scale(&self, anatomy: &HumanAnatomy) -> f64
    {
        self.possible.iter().filter_map(|x| anatomy.body.get(*x).map(|x| x.size)).sum()
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

        let new_part_with_contents = |name, health, size, contents|
        {
            HumanPart::new(
                name,
                part.clone(),
                bone_toughness * health,
                size,
                contents
            )
        };

        let new_part = |name, health, size|
        {
            new_part_with_contents(name, health, size, Vec::new())
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
            let foot = Some(new_part(DebugName::new(with_name("foot")), 5000.0, 0.17));

            let upper_arm = Some(new_part(DebugName::new(with_name("upper arm")), 2500.0, 0.2));
            let lower_arm = Some(new_part(DebugName::new(with_name("lower arm")), 2000.0, 0.17));
            let hand = Some(new_part(DebugName::new(with_name("hand")), 4000.0, 0.07));

            let eye = Some(HumanPart::new_full(
                DebugName::new(with_name("eye")),
                Health::new(50.0, 100.0),
                None,
                None,
                0.01,
                Vec::new()
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
            5000.0,
            0.39,
            vec![HumanOrgan::Brain(Brain::default())]
        );

        let pelvis = new_part(DebugName::new("pelvis"), 6000.0, 0.37);
        let torso = new_part_with_contents(
            DebugName::new("torso"),
            3300.0,
            0.82,
            vec![
                HumanOrgan::Lung(Lung::left()),
                HumanOrgan::Lung(Lung::right())
            ]
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

    pub fn is_crawling(&self) -> bool
    {
        self.cached.is_crawling
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        self.base_speed = speed;

        self.update_cache();
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

        let mut ids = match damage.direction.height
        {
            DamageHeight::Top =>
            {
                match damage.direction.side
                {
                    Side2d::Back => vec![
                        (HumanPartId::Spine, no_pierce()),
                        (HumanPartId::Head, PierceType::head_back())
                    ],
                    Side2d::Front => vec![
                        (HumanPartId::Spine, no_pierce()),
                        (HumanPartId::Head, no_pierce()),
                        (HumanPartId::Eye(Side1d::Left), no_pierce()),
                        (HumanPartId::Eye(Side1d::Right), no_pierce())
                    ],
                    Side2d::Left | Side2d::Right => vec![
                        (HumanPartId::Spine, no_pierce()),
                        (HumanPartId::Head, no_pierce())
                    ]
                }
            },
            DamageHeight::Middle =>
            {
                match damage.direction.side
                {
                    Side2d::Back | Side2d::Front => vec![
                        (HumanPartId::Spine, no_pierce()),
                        (HumanPartId::Torso, no_pierce()),
                        (HumanPartId::UpperArm(Side1d::Left), no_pierce()),
                        (HumanPartId::LowerArm(Side1d::Left), no_pierce()),
                        (HumanPartId::Hand(Side1d::Left), no_pierce()),
                        (HumanPartId::UpperArm(Side1d::Right), no_pierce()),
                        (HumanPartId::LowerArm(Side1d::Right), no_pierce()),
                        (HumanPartId::Hand(Side1d::Right), no_pierce())
                    ],
                    Side2d::Left => vec![
                        (HumanPartId::Spine, PierceType::middle_pierce(Side1d::Left)),
                        (HumanPartId::Torso, PierceType::middle_pierce(Side1d::Left)),
                        (HumanPartId::UpperArm(Side1d::Left), PierceType::arm_pierce(Side1d::Left)),
                        (HumanPartId::LowerArm(Side1d::Left), PierceType::arm_pierce(Side1d::Left)),
                        (HumanPartId::Hand(Side1d::Left), PierceType::arm_pierce(Side1d::Left))
                    ],
                    Side2d::Right => vec![
                        (HumanPartId::Spine, PierceType::middle_pierce(Side1d::Right)),
                        (HumanPartId::Torso, PierceType::middle_pierce(Side1d::Right)),
                        (HumanPartId::UpperArm(Side1d::Right), PierceType::arm_pierce(Side1d::Right)),
                        (HumanPartId::LowerArm(Side1d::Right), PierceType::arm_pierce(Side1d::Right)),
                        (HumanPartId::Hand(Side1d::Right), PierceType::arm_pierce(Side1d::Right))
                    ]
                }
            },
            DamageHeight::Bottom =>
            {
                match damage.direction.side
                {
                    Side2d::Back | Side2d::Front => vec![
                        (HumanPartId::UpperLeg(Side1d::Left), no_pierce()),
                        (HumanPartId::LowerLeg(Side1d::Left), no_pierce()),
                        (HumanPartId::Foot(Side1d::Left), no_pierce()),
                        (HumanPartId::UpperLeg(Side1d::Right), no_pierce()),
                        (HumanPartId::LowerLeg(Side1d::Right), no_pierce()),
                        (HumanPartId::Foot(Side1d::Right), no_pierce())
                    ],
                    Side2d::Left => vec![
                        (HumanPartId::UpperLeg(Side1d::Left), PierceType::leg_pierce(Side1d::Left)),
                        (HumanPartId::LowerLeg(Side1d::Left), PierceType::leg_pierce(Side1d::Left)),
                        (HumanPartId::Foot(Side1d::Left), PierceType::leg_pierce(Side1d::Left))
                    ],
                    Side2d::Right => vec![
                        (HumanPartId::UpperLeg(Side1d::Right), PierceType::leg_pierce(Side1d::Right)),
                        (HumanPartId::LowerLeg(Side1d::Right), PierceType::leg_pierce(Side1d::Right)),
                        (HumanPartId::Foot(Side1d::Right), PierceType::leg_pierce(Side1d::Right))
                    ]
                }
            }
        };

        ids.retain(|(id, pierce)|
        {
            self.body.get(*id).is_some() || pierce.any_exists(self)
        });

        let ids: &Vec<_> = &ids;

        let picked = WeightedPicker::pick_from(
            damage.rng.next_f64(),
            ids,
            |(id, pierce)|
            {
                self.body.get(*id).map(|x| x.size).unwrap_or_else(|| pierce.combined_scale(self))
            }
        );

        let pierce = picked.and_then(|(picked, on_pierce)|
        {
            if let Some(main_pick) = self.body.get_mut(*picked)
            {
                main_pick.damage(damage).and_then(|pierce|
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
        body.sided.as_ref().zip(motor).map(|(body, motor)|
        {
            Speeds{
                legs: Self::leg_speed(body) * motor.legs,
                arms: Self::arm_speed(body) * motor.arms
            }
        }).combine(|a, b| Speeds{legs: a.legs + b.legs, arms: a.arms + b.arms})
    }

    fn brain(&self) -> Option<&Brain>
    {
        self.body.head.contents.iter()
            .find_map(|x| if let HumanOrgan::Brain(x) = x { Some(x) } else { None })
    }

    fn updated_speed(&mut self) -> (bool, Option<f32>)
    {
        let brain = some_or_value!(self.brain(), (false, None));

        let speeds = brain.as_ref().map(|hemisphere|
        {
            Speeds{
                arms: hemisphere.frontal.motor.arms.fraction(),
                legs: hemisphere.frontal.motor.legs.fraction()
            }
        });

        let Speeds{arms, legs} = Self::speed_scale(&self.body, speeds);

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
    fn damage(&mut self, mut damage: Damage) -> Option<Damage>
    {
        if self.is_crawling()
        {
            damage = damage.scale(2.0);
        }

        self.damage_random_part(damage)
    }
}
