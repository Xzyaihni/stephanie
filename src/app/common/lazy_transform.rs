use std::f32;

use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3, Rotation as NRotation};

use yanyaengine::Transform;

use crate::common::{
    lerp,
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

pub trait LazyTargettable
{
    fn target(&mut self) -> &mut Transform;
}

pub struct LazyTransformInfo
{
    pub connection: Connection,
    pub rotation: Rotation,
    pub deformation: Deformation,
    pub origin_rotation: f32,
    pub origin: Vector3<f32>,
    pub transform: Transform
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazyTransformServer
{
    pub target_local: Transform,
    origin_rotation: f32,
    origin: Vector3<f32>,
    connection: Connection,
    rotation: Rotation,
    deformation: Deformation
}

impl From<LazyTransformInfo> for LazyTransformServer
{
    fn from(info: LazyTransformInfo) -> Self
    {
        Self{
            target_local: info.transform,
            origin_rotation: info.origin_rotation,
            origin: info.origin,
            connection: info.connection,
            rotation: info.rotation,
            deformation: info.deformation
        }
    }
}

impl LazyTargettable for LazyTransformServer
{
    fn target(&mut self) -> &mut Transform
    {
        &mut self.target_local
    }
}

#[derive(Debug, Clone)]
pub struct LazyTransform
{
    pub target_local: Transform,
    current: Transform,
    origin_rotation: f32,
    origin: Vector3<f32>,
    connection: Connection,
    rotation: Rotation,
    deformation: Deformation
}

impl From<LazyTransformInfo> for LazyTransform
{
    fn from(info: LazyTransformInfo) -> Self
    {
        Self{
            target_local: info.transform.clone(),
            current: info.transform,
            origin_rotation: info.origin_rotation,
            origin: info.origin,
            connection: info.connection,
            rotation: info.rotation,
            deformation: info.deformation
        }
    }
}

impl LazyTargettable for LazyTransform
{
    fn target(&mut self) -> &mut Transform
    {
        &mut self.target_local
    }
}

impl LazyTransform
{
    pub fn next(
        &mut self,
        parent_transform: Option<Transform>,
        dt: f32
    ) -> Transform
    {
        let target_global = Self::target_global(
            self.target_local.clone(),
            parent_transform.as_ref()
        );

        let mut current = self.current.clone();

        current.scale = target_global.scale;

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

        match &mut self.connection
        {
            Connection::Rigid =>
            {
                current.position = target_global.position;
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

                current.position.z = target_global.position.z;
            }
        }

        match &self.deformation
        {
            Deformation::Rigid => (),
            Deformation::Stretch(deformation) =>
            {
                let velocity = self.physical().map(|x| x.velocity).unwrap_or_else(Vector3::zeros);

                current.stretch = deformation.stretch(velocity);
            }
        }

        self.current = current.clone();

        if let Some(parent) = parent_transform
        {
            let rotation = NRotation::from_axis_angle(
                &current.rotation_axis,
                current.rotation + self.origin_rotation
            );

            let relative_position = current.position - parent.position;

            let origin = self.origin.component_mul(&target_global.scale);
            let offset_position = relative_position - origin;
            current.position = rotation * offset_position + parent.position;
        }

        current
    }

    pub fn combine(&self, parent: &Transform) -> Transform
    {
        Self::combine_parent(self.target_local.clone(), parent)
    }

    pub fn combine_parent(mut transform: Transform, parent: &Transform) -> Transform
    {
        transform.position += parent.position;
        transform.rotation += parent.rotation;
        transform.scale.component_mul_assign(&parent.scale);

        transform
    }

    pub fn target_global(transform: Transform, parent: Option<&Transform>) -> Transform
    {
        if let Some(parent) = parent
        {
            Self::combine_parent(transform, parent)
        } else
        {
            transform
        }
    }

    pub fn reset_current(&mut self, target: Transform)
    {
        self.current = target;
    }

    pub fn from_server(transform: Transform, info: LazyTransformServer) -> Self
    {
        Self{
            target_local: info.target_local,
            current: transform,
            origin_rotation: info.origin_rotation,
            origin: info.origin,
            connection: info.connection,
            rotation: info.rotation,
            deformation: info.deformation
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
