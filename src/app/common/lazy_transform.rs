use std::f32;

use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3, Rotation as NRotation};

use yanyaengine::Transform;

use crate::common::{
    lerp,
    Parent,
    Physical
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValueAnimation
{
    Linear,
    EaseIn(f32),
    EaseOut(f32)
}

impl ValueAnimation
{
    pub fn apply(&self, value: f32) -> f32
    {
        let value = value.clamp(0.0, 1.0);

        match self
        {
            Self::Linear => value,
            Self::EaseIn(strength) => value.powf(*strength),
            Self::EaseOut(strength) => 1.0 - (1.0 - value).powf(*strength)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringConnection
{
    pub limit: f32,
    pub damping: f32,
    pub strength: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EaseOutRotation
{
    pub resistance: f32,
    pub momentum: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstantRotation
{
    pub speed: f32,
    pub momentum: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationInfo<T>
{
    last_move: f32,
    props: T
}

impl<T> From<T> for RotationInfo<T>
{
    fn from(props: T) -> Self
    {
        Self{
            last_move: 0.0,
            props
        }
    }
}

pub type EaseOutRotationInfo = RotationInfo<EaseOutRotation>;
pub type ConstantRotationInfo = RotationInfo<ConstantRotation>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StretchDeformation
{
    pub animation: ValueAnimation,
    pub limit: f32,
    pub onset: f32,
    pub strength: f32
}

impl StretchDeformation
{
    pub fn stretch(&self, velocity: Vector3<f32>) -> (f32, Vector2<f32>)
    {
        let amount = self.animation.apply(velocity.magnitude() * self.onset);
        let stretch = (1.0 + amount * self.strength).max(self.limit);

        let angle = velocity.y.atan2(-velocity.x);

        (angle, Vector2::new(stretch, 1.0 / stretch))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Connection
{
    Rigid,
    Spring(SpringConnection)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Rotation
{
    Instant,
    EaseOut(EaseOutRotationInfo),
    Constant(ConstantRotationInfo)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Deformation
{
    Rigid,
    Stretch(StretchDeformation)
}

pub struct LazyTransformInfo
{
    pub connection: Connection,
    pub rotation: Rotation,
    pub deformation: Deformation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazyTransform
{
    pub target: Transform,
    current: Transform,
    connection: Connection,
    rotation: Rotation,
    deformation: Deformation
}

impl From<LazyTransformInfo> for LazyTransform
{
    fn from(info: LazyTransformInfo) -> Self
    {
        Self{
            target: Default::default(),
            current: Default::default(),
            connection: info.connection,
            rotation: info.rotation,
            deformation: info.deformation
        }
    }
}

impl LazyTransform
{
    pub fn next(
        &mut self,
        parent: Option<&Parent>,
        physical: &mut Physical,
        dt: f32
    ) -> Transform
    {
        let mut current = self.current.clone();

        match &self.rotation
        {
            Rotation::Instant =>
            {
                current.rotation = self.target.rotation;
            },
            Rotation::EaseOut(..) | Rotation::Constant{..} =>
            {
                let pi2 = 2.0 * f32::consts::PI;
                let rotation_difference = (self.target.rotation - current.rotation) % pi2;

                let short_difference = if rotation_difference > f32::consts::PI
                {
                    rotation_difference - 2.0 * f32::consts::PI
                } else if rotation_difference < -f32::consts::PI
                {
                    rotation_difference + 2.0 * f32::consts::PI
                } else
                {
                    rotation_difference
                };

                let half = -f32::consts::PI..f32::consts::PI;
                let long_difference = if half.contains(&rotation_difference)
                {
                    if rotation_difference < 0.0
                    {
                        (2.0 * f32::consts::PI) + rotation_difference
                    } else
                    {
                        (-2.0 * f32::consts::PI) + rotation_difference
                    }
                } else
                {
                    rotation_difference
                };

                let short_fraction = -short_difference / long_difference;

                let current_difference = |last_move: f32, momentum: f32|
                {
                    #[allow(clippy::collapsible_else_if)]
                    if (last_move * short_difference).is_sign_positive()
                    {
                        // was moving in the shortest direction already
                        short_difference
                    } else
                    {
                        if (1.0 - momentum) < short_fraction
                        {
                            long_difference
                        } else
                        {
                            short_difference
                        }
                    }
                };

                let rotation = current.rotation;

                match &mut self.rotation
                {
                    Rotation::EaseOut(info) =>
                    {
                        let amount = 1.0 - info.props.resistance.powf(dt);

                        let current_difference =
                            current_difference(info.last_move, info.props.momentum);

                        let target_rotation = current_difference + rotation;

                        let new_rotation = lerp(rotation, target_rotation, amount);

                        info.last_move = new_rotation - rotation;

                        current.rotation = new_rotation;
                    },
                    Rotation::Constant(info) =>
                    {
                        let max_move = info.props.speed * dt;

                        let current_difference =
                            current_difference(info.last_move, info.props.momentum);

                        let move_amount = current_difference.clamp(-max_move, max_move);

                        info.last_move = move_amount;

                        let new_rotation = rotation + move_amount;

                        current.rotation = new_rotation;
                    },
                    _ => unreachable!()
                }
            }
        }

        match &self.connection
        {
            Connection::Rigid =>
            {
                current.position = self.target.position;
            },
            Connection::Spring(connection) =>
            {
                let distance = self.target.position - current.position;

                let spring_force = distance * connection.strength;

                physical.force += spring_force;
                physical.damp_velocity(connection.damping, dt);

                current.position = self.clamp_distance(current.position, connection.limit);

                current.position.z = self.target.position.z;
            }
        }

        match &self.deformation
        {
            Deformation::Rigid => (),
            Deformation::Stretch(deformation) =>
            {
                current.stretch = deformation.stretch(physical.velocity);
            }
        }

        self.current = current.clone();

        if let Some(parent) = parent
        {
            let rotation = NRotation::from_axis_angle(
                &current.rotation_axis,
                current.rotation + parent.origin_rotation()
            );

            let original_position = parent.child_transform().position;

            let origin = parent.origin().component_mul(&self.target.scale);
            let offset_position = original_position - origin;
            current.position += rotation * offset_position - original_position;
        }

        current
    }

    pub fn reset_current(&mut self)
    {
        self.current = self.target.clone();
    }

    fn clamp_distance(&mut self, current: Vector3<f32>, limit: f32) -> Vector3<f32>
    {
        let distance = self.target.position - current;

        // checking again cuz this is after the physics update
        if distance.magnitude() < limit
        {
            return current;
        }

        let limited_position = distance.normalize() * limit;

        self.target.position - limited_position
    }
}
