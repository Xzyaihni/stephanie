use std::f32;

use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3, Rotation as NRotation};

use yanyaengine::Transform;

use crate::common::{
    ease_out,
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
    pub physical: Physical,
    pub limit: f32,
    pub damping: f32,
    pub strength: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EaseOutRotation
{
    pub decay: f32,
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
    pub fn stretch(&self, rotation: f32, velocity: Vector3<f32>) -> (f32, Vector2<f32>)
    {
        let amount = self.animation.apply(velocity.magnitude() * self.onset);
        let stretch = (1.0 + amount * self.strength).min(self.limit);

        let angle = velocity.y.atan2(-velocity.x) + rotation;

        (angle, Vector2::new(stretch, 1.0 / stretch))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Connection
{
    Rigid,
    Limit{limit: f32},
    Constant{speed: f32},
    EaseOut{decay: f32, limit: f32},
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
pub enum Scaling
{
    Instant,
    EaseOut{decay: f32},
    Constant{speed: f32}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Deformation
{
    Rigid,
    Stretch(StretchDeformation)
}

pub trait LazyTargettable<T=Transform>
{
    fn target_ref(&self) -> &T;
    fn target(&mut self) -> &mut T;
}

pub struct LazyTransformInfo
{
    pub connection: Connection,
    pub rotation: Rotation,
    pub scaling: Scaling,
    pub deformation: Deformation,
    pub origin_rotation: f32,
    pub origin: Vector3<f32>,
    pub inherit_scale: bool,
    pub transform: Transform
}

impl Default for LazyTransformInfo
{
    fn default() -> Self
    {
        Self{
            connection: Connection::Rigid,
            rotation: Rotation::Instant,
            scaling: Scaling::Instant,
            deformation: Deformation::Rigid,
            origin_rotation: 0.0,
            origin: Vector3::zeros(),
            inherit_scale: true,
            transform: Transform::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazyTransform
{
    pub target_local: Transform,
    origin_rotation: f32,
    origin: Vector3<f32>,
    inherit_scale: bool,
    pub connection: Connection,
    pub rotation: Rotation,
    pub scaling: Scaling,
    pub deformation: Deformation
}

impl LazyTargettable for LazyTransform
{
    fn target_ref(&self) -> &Transform
    {
        &self.target_local
    }

    fn target(&mut self) -> &mut Transform
    {
        &mut self.target_local
    }
}

impl From<LazyTransformInfo> for LazyTransform
{
    fn from(info: LazyTransformInfo) -> Self
    {
        Self{
            target_local: info.transform,
            origin_rotation: info.origin_rotation,
            origin: info.origin,
            inherit_scale: info.inherit_scale,
            connection: info.connection,
            rotation: info.rotation,
            scaling: info.scaling,
            deformation: info.deformation
        }
    }
}

impl LazyTransform
{
    pub fn next(
        &mut self,
        physical: Option<&Physical>,
        mut current: Transform,
        parent_transform: Option<Transform>,
        dt: f32
    ) -> Transform
    {
        let mut target_global = self.target_global(parent_transform.as_ref());

        let constant_change = |current: &mut Vector3<f32>, target: Vector3<f32>, speed|
        {
            let max_move = Vector3::repeat(speed * dt);

            let current_difference = target - *current;

            let move_amount = current_difference.zip_map(&max_move, |diff, limit: f32|
            {
                diff.clamp(-limit, limit)
            });

            *current += move_amount;
        };

        let ease_out_change = |current: &mut Vector3<f32>, target: Vector3<f32>, decay: f32|
        {
            *current = current.zip_map(&target, |a, b|
            {
                ease_out(a, b, decay, dt)
            });
        };

        match &self.scaling
        {
            Scaling::Instant =>
            {
                current.scale = target_global.scale;
            },
            Scaling::Constant{speed} =>
            {
                constant_change(&mut current.scale, target_global.scale, speed)
            },
            Scaling::EaseOut{decay} =>
            {
                ease_out_change(&mut current.scale, target_global.scale, *decay)
            }
        }

        match &self.rotation
        {
            Rotation::Instant =>
            {
                current.rotation = target_global.rotation;
            },
            Rotation::EaseOut(..) | Rotation::Constant{..} =>
            {
                let pi2 = 2.0 * f32::consts::PI;
                let rotation_difference = (target_global.rotation - current.rotation) % pi2;

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
                        let current_difference =
                            current_difference(info.last_move, info.props.momentum);

                        let target_rotation = current_difference + rotation;

                        let new_rotation = ease_out(
                            rotation,
                            target_rotation,
                            info.props.decay,
                            dt
                        );

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

        let rotation = NRotation::from_axis_angle(
            &current.rotation_axis,
            current.rotation + self.origin_rotation
        );

        if let Some(parent) = parent_transform
        {
            let scaled_origin = self.origin.component_mul(&parent.scale);
            let offset_position =
                self.target_local.position.component_mul(&parent.scale) - scaled_origin;

            target_global.position = rotation * offset_position + parent.position + scaled_origin;
        }

        match &mut self.connection
        {
            Connection::Rigid =>
            {
                current.position = target_global.position;
            },
            Connection::Constant{speed} =>
            {
                constant_change(&mut current.position, target_global.position, speed)
            },
            Connection::Limit{limit} =>
            {
                current.position = Self::clamp_distance(
                    target_global.position,
                    current.position,
                    *limit
                );
            },
            Connection::EaseOut{decay, limit} =>
            {
                ease_out_change(&mut current.position, target_global.position, *decay);

                current.position = Self::clamp_distance(
                    target_global.position,
                    current.position,
                    *limit
                );
            },
            Connection::Spring(connection) =>
            {
                let distance = target_global.position - current.position;

                let spring_force = distance * connection.strength;

                connection.physical.force += spring_force;
                connection.physical.damp_velocity(connection.damping, dt);
                connection.physical.physics_update(&mut current, dt);

                current.position = Self::clamp_distance(
                    target_global.position,
                    current.position,
                    connection.limit
                );
            }
        }

        current.position.z = target_global.position.z;

        match &self.deformation
        {
            Deformation::Rigid => (),
            Deformation::Stretch(deformation) =>
            {
                let local_velocity = self.physical().map(|x| x.velocity)
                    .unwrap_or_else(Vector3::zeros);

                let global_velocity = physical.map(|x| x.velocity)
                    .unwrap_or_else(Vector3::zeros);

                let velocity = global_velocity + local_velocity;

                current.stretch = deformation.stretch(current.rotation, velocity);
            }
        }

        current
    }

    pub fn set_connection_limit(&mut self, new_limit: f32)
    {
        match &mut self.connection
        {
            Connection::Rigid{..} => (),
            Connection::Constant{..} => (),
            Connection::Limit{limit} =>
            {
                *limit = new_limit;
            },
            Connection::EaseOut{limit, ..} =>
            {
                *limit = new_limit;
            },
            Connection::Spring(connection) =>
            {
                connection.limit = new_limit;
            }
        }
    }

    pub fn combine(&self, parent: &Transform) -> Transform
    {
        let mut transform = self.target_local.clone();

        transform.position = transform.position.component_mul(&parent.scale) + parent.position;
        transform.rotation += parent.rotation;
        transform.rotation %= f32::consts::PI * 2.0;

        if self.inherit_scale
        {
            transform.scale.component_mul_assign(&parent.scale);
        }

        transform
    }

    pub fn target_global(
        &self,
        parent: Option<&Transform>
    ) -> Transform
    {
        if let Some(parent) = parent
        {
            self.combine(parent)
        } else
        {
            self.target_local.clone()
        }
    }

    fn physical(&self) -> Option<&Physical>
    {
        match &self.connection
        {
            Connection::Spring(x) => Some(&x.physical),
            _ => None
        }
    }

    pub fn origin_rotation(&self) -> f32
    {
        self.origin_rotation
    }

    fn clamp_distance(target: Vector3<f32>, current: Vector3<f32>, limit: f32) -> Vector3<f32>
    {
        let distance = target - current;

        // checking again cuz this is after the physics update
        if distance.magnitude() < limit
        {
            return current;
        }

        let limited_position = distance.normalize() * limit;

        target - limited_position
    }
}
