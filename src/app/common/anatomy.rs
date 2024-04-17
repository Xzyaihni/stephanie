use std::ops::{Index, IndexMut};

use serde::{Serialize, Deserialize};


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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bone<Data>
{
    data: Data,
    children: Vec<BodyPart<Bone<Data>>>
}

impl<Data> Bone<Data>
{
    pub fn new(data: Data, children: Vec<BodyPart<Bone<Data>>>) -> Self
    {
        Self{data, children}
    }

    pub fn leaf(data: Data) -> Self
    {
        Self::new(data, Vec::new())
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
    part: Data
}

impl<Data> BodyPart<Data>
{
    pub fn new(bone: impl Into<Health>, part: Data) -> Self
    {
        Self::new_full(bone, 100.0, 500.0, part)
    }

    pub fn new_full(
        bone: impl Into<Health>,
        skin: impl Into<Health>,
        muscle: impl Into<Health>,
        part: Data
    ) -> Self
    {
        Self{bone: bone.into(), skin: skin.into(), muscle: muscle.into(), part}
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Side
{
    Left,
    Right
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

struct Speeds
{
    arms: f32,
    legs: f32
}

struct SpeedsState
{
    conseq_leg: u32,
    conseq_arm: u32,
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
            HumanBone::new(
                HumanBoneSingle::Femur,
                vec![
                    HumanPart::new(
                        3500.0,
                        HumanBone::new(
                            HumanBoneSingle::Tibia,
                            vec![
                                HumanPart::new(
                                    5000.0,
                                    HumanBone::leaf(HumanBoneSingle::Foot)
                                )
                            ]
                        )
                    )
                ]
            )
        );

        let arm = HumanPart::new(
            2500.0,
            HumanBone::new(
                HumanBoneSingle::Humerus,
                vec![
                    HumanPart::new(
                        2000.0,
                        HumanBone::new(
                            HumanBoneSingle::Radius,
                            vec![
                                HumanPart::new(
                                    4000.0,
                                    HumanBone::leaf(HumanBoneSingle::Hand)
                                )
                            ]
                        )
                    )
                ]
            )
        );

        let body = HumanPart::new(
            3400.0,
            HumanBone::new(
                HumanBoneSingle::Spine,
                vec![
                    HumanPart::new(
                        5000.0,
                        HumanBone::leaf(HumanBoneSingle::Skull{contents: vec![
                            HumanOrgan::Brain(Brain::default()),
                            HumanOrgan::Eye(Eye::left()),
                            HumanOrgan::Eye(Eye::right())
                        ]})
                    ),
                    HumanPart::new(
                        6000.0,
                        HumanBone::new(
                            HumanBoneSingle::Pelvis,
                            vec![
                                leg.clone(),
                                leg
                            ]
                        )
                    ),
                    HumanPart::new(
                        3300.0,
                        HumanBone::new(
                            HumanBoneSingle::Ribcage{contents: vec![
                                HumanOrgan::Lung(Lung::left()),
                                HumanOrgan::Lung(Lung::right())
                            ]},
                            vec![
                                arm.clone(),
                                arm
                            ]
                        )
                    )
                ]
            )
        );

        let mut this = Self{
            base_speed: 0.5,
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
    }

    fn leg_speeds(conseq: u32, health_mult: f32, part: &HumanPart) -> (u32, f32, Option<f32>)
    {
        let bone = &part.part;

        match bone.data
        {
            HumanBoneSingle::Femur =>
            {
                (1, 0.1, Some(health_mult))
            },
            HumanBoneSingle::Tibia =>
            {
                let base = 0.05;
                if conseq == 1
                {
                    (2, base, Some(health_mult * 2.0))
                } else
                {
                    (0, base, Some(health_mult * 0.9))
                }
            },
            HumanBoneSingle::Foot =>
            {
                let base = 0.01;
                if bone.children.is_empty()
                {
                    if conseq == 2
                    {
                        (0, base, Some(health_mult * 3.0))
                    } else
                    {
                        (0, base, Some(health_mult * 0.5))
                    }
                } else
                {
                    (0, base, None)
                }
            },
            _ => (0, 0.0, None)
        }
    }

    fn arm_speeds(conseq: u32, health_mult: f32, part: &HumanPart) -> (u32, f32, Option<f32>)
    {
        let bone = &part.part;

        match bone.data
        {
            HumanBoneSingle::Humerus =>
            {
                (1, 0.1, Some(health_mult))
            },
            HumanBoneSingle::Radius =>
            {
                let base = 0.05;
                if conseq == 1
                {
                    (2, base, Some(health_mult * 1.5))
                } else
                {
                    (0, base, Some(health_mult * 0.9))
                }
            },
            HumanBoneSingle::Hand =>
            {
                let base = 0.01;
                if bone.children.is_empty()
                {
                    if conseq == 2
                    {
                        (0, base, Some(health_mult * 2.0))
                    } else
                    {
                        (0, base, Some(health_mult * 0.2))
                    }
                } else
                {
                    (0, base, None)
                }
            },
            _ => (0, 0.0, None)
        }
    }

    fn speed_scale(state: &mut SpeedsState, part: &HumanPart) -> Speeds
    {
        let health_mult = (part.bone.fraction() * 0.9 + 0.1) * part.muscle.fraction();

        let (conseq_leg, base_leg, mult_leg) = Self::leg_speeds(
            state.conseq_leg,
            health_mult,
            part
        );

        state.conseq_leg = conseq_leg;

        let (conseq_arm, base_arm, mult_arm) = Self::arm_speeds(
            state.conseq_arm,
            health_mult,
            part
        );

        state.conseq_arm = conseq_arm;

        let bone = &part.part;

        let children_speed: Speeds = bone.children.iter().map(|child|
        {
            Self::speed_scale(state, child)
        }).reduce(|acc, x| Speeds{arms: acc.arms + x.arms, legs: acc.legs + x.legs})
            .unwrap_or_else(|| Speeds{arms: 0.0, legs: 0.0});

        let leg_speed = if let Some(mult_leg) = mult_leg
        {
            mult_leg * children_speed.legs
        } else
        {
            children_speed.legs
        };

        let arm_speed = if let Some(mult_arm) = mult_arm
        {
            mult_arm * children_speed.arms
        } else
        {
            children_speed.arms
        };

        Speeds{
            arms: arm_speed + base_arm,
            legs: leg_speed + base_leg
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

        bone.children.iter().find_map(|part| Self::brain_search(&part.part))
    }

    fn brain(&self) -> Option<&Brain>
    {
        Self::brain_search(&self.body.part)
    }

    fn updated_speed(&mut self) -> Option<f32>
    {
        let brain = if let Some(brain) = self.brain()
        {
            brain
        } else
        {
            return None;
        };

        let mut state = SpeedsState{
            conseq_leg: 0,
            conseq_arm: 0,
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
            Some(self.base_speed)
        }
    }

    fn update(&mut self)
    {
        self.cached.speed = self.updated_speed();
    }
}
