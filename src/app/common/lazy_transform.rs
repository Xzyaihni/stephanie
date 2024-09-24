use std::f32;

use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector2, Vector3, Rotation as NRotation};

use yanyaengine::Transform;

use crate::common::{
    short_rotation,
    lerp,
    EaseOut,
    Entity,
    Physical,
    ColliderType,
    watcher::Lifetime
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
    pub strength: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EaseOutRotation
{
    pub decay: f32,
    pub speed_significant: f32,
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

impl EaseOutRotationInfo
{
    pub fn set_decay(&mut self, decay: f32)
    {
        self.props.decay = decay;
    }
}

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
        let amount = self.animation.apply(velocity.xy().magnitude() * self.onset);
        let stretch = (1.0 + amount * self.strength).min(self.limit);

        let angle = velocity.y.atan2(-velocity.x) + rotation;

        (angle, Vector2::new(stretch, 1.0 / stretch))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedConnection
{
    lifetime: Lifetime,
    start: Option<Vector3<f32>>
}

impl From<Lifetime> for TimedConnection
{
    fn from(lifetime: Lifetime) -> Self
    {
        Self{lifetime, start: None}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Connection
{
    Ignore,
    Rigid,
    Limit{limit: f32},
    Timed(TimedConnection),
    Constant{speed: f32},
    EaseOut{decay: f32, limit: Option<f32>},
    Spring(SpringConnection)
}

impl Connection
{
    fn next(
        &mut self,
        current: &mut Transform,
        target: Vector3<f32>,
        dt: f32
    )
    {
        match self
        {
            Connection::Ignore => (),
            Connection::Rigid =>
            {
                current.position = target;
            },
            Connection::Timed(TimedConnection{lifetime, ref mut start}) =>
            {
                if start.is_none()
                {
                    *start = Some(current.position);
                }

                let remaining = 1.0 - lifetime.fraction();

                current.position = start.unwrap().zip_map(&target, |a, b|
                {
                    lerp(a, b, remaining)
                });

                lifetime.current -= dt;
            },
            Connection::Constant{speed} =>
            {
                let max_move = Vector3::repeat(*speed * dt);

                let current_difference = target - current.position;

                let move_amount = current_difference.zip_map(&max_move, |diff, limit: f32|
                {
                    diff.clamp(-limit, limit)
                });

                current.position += move_amount;
            },
            Connection::Limit{limit} =>
            {
                current.position = LazyTransform::clamp_distance(
                    target,
                    current.position,
                    *limit
                );
            },
            Connection::EaseOut{decay, limit} =>
            {
                current.position = current.position.ease_out(target, *decay, dt);

                if let Some(limit) = limit
                {
                    current.position = LazyTransform::clamp_distance(
                        target,
                        current.position,
                        *limit
                    );
                }
            },
            Connection::Spring(connection) =>
            {
                let distance = target - current.position;

                let spring_force = distance * connection.strength;

                connection.physical.add_force(spring_force);
                connection.physical.update(
                    current,
                    |physical, transform| ColliderType::Circle.inverse_inertia(physical, transform),
                    dt
                );

                current.position = LazyTransform::clamp_distance(
                    target,
                    current.position,
                    connection.limit
                );
            }
        }

        current.position.z = target.z;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Rotation
{
    Ignore,
    Instant,
    EaseOut(EaseOutRotationInfo),
    Constant(ConstantRotationInfo)
}

impl Rotation
{
    fn next(
        &mut self,
        current: &mut f32,
        target: f32,
        dt: f32
    )
    {
        match &self
        {
            Rotation::Ignore => (),
            Rotation::Instant =>
            {
                *current = target;
            },
            Rotation::EaseOut(..) | Rotation::Constant{..} =>
            {
                let rotation_difference = target - *current;

                let short_difference = short_rotation(rotation_difference);

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

                let current_difference = |last_move: f32, momentum: f32, speed_significant: f32|
                {
                    #[allow(clippy::collapsible_else_if)]
                    if (last_move * short_difference).is_sign_positive()
                    {
                        // was moving in the shortest direction already
                        short_difference
                    } else
                    {
                        let below_momentum = (1.0 - momentum) < short_fraction;

                        if below_momentum && speed_significant < last_move
                        {
                            long_difference
                        } else
                        {
                            short_difference
                        }
                    }
                };

                let rotation = *current;

                match self
                {
                    Rotation::EaseOut(info) =>
                    {
                        let current_difference = current_difference(
                            info.last_move,
                            info.props.momentum,
                            info.props.speed_significant
                        );

                        let target_rotation = current_difference + rotation;

                        let new_rotation = rotation.ease_out(
                            target_rotation,
                            info.props.decay,
                            dt
                        );

                        info.last_move = new_rotation - rotation;

                        *current = new_rotation;
                    },
                    Rotation::Constant(info) =>
                    {
                        let max_move = info.props.speed * dt;

                        let current_difference =
                            current_difference(info.last_move, info.props.momentum, 0.0);

                        let move_amount = current_difference.clamp(-max_move, max_move);

                        info.last_move = move_amount;

                        let new_rotation = rotation + move_amount;

                        *current = new_rotation;
                    },
                    _ => unreachable!()
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Scaling
{
    Ignore,
    Instant,
    EaseOut{decay: f32},
    Constant{speed: f32}
}

impl Scaling
{
    fn next(
        &mut self,
        current: &mut Vector3<f32>,
        target: Vector3<f32>,
        dt: f32
    )
    {
        match &self
        {
            Scaling::Ignore => (),
            Scaling::Instant =>
            {
                *current = target;
            },
            Scaling::Constant{speed} =>
            {
                let max_move = Vector3::repeat(speed * dt);

                let current_difference = target - *current;

                let move_amount = current_difference.zip_map(&max_move, |diff, limit: f32|
                {
                    diff.clamp(-limit, limit)
                });

                *current += move_amount;
            },
            Scaling::EaseOut{decay} =>
            {
                *current = current.ease_out(target, *decay, dt);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Deformation
{
    Rigid,
    Stretch(StretchDeformation)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowRotation
{
    parent: Entity,
    pub rotation: Rotation
}

impl FollowRotation
{
    pub fn new(parent: Entity, rotation: Rotation) -> Self
    {
        Self{
            parent,
            rotation
        }
    }

    pub fn parent(&self) -> Entity
    {
        self.parent
    }

    pub fn next(
        &mut self,
        current: &mut f32,
        parent_rotation: f32,
        dt: f32
    )
    {
        self.rotation.next(current, parent_rotation, dt);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowPosition
{
    parent: Entity,
    pub connection: Connection,
    pub offset: Vector3<f32>
}

impl FollowPosition
{
    pub fn new(parent: Entity, connection: Connection) -> Self
    {
        Self{
            parent,
            connection,
            offset: Vector3::zeros()
        }
    }

    pub fn parent(&self) -> Entity
    {
        self.parent
    }

    pub fn next(
        &mut self,
        current: &mut Transform,
        parent_position: Vector3<f32>,
        dt: f32
    )
    {
        self.connection.next(current, parent_position + self.offset, dt);
    }
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
    pub inherit_rotation: bool,
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
            inherit_rotation: true,
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
    inherit_rotation: bool,
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
            inherit_rotation: info.inherit_rotation,
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
        let mut target_global = self.target_global_unrotated(parent_transform.as_ref());

        let pi2 = 2.0 * f32::consts::PI;
        current.rotation %= pi2;
        target_global.rotation %= pi2;

        self.scaling.next(&mut current.scale, target_global.scale, dt);
        self.rotation.next(&mut current.rotation, target_global.rotation, dt);

        self.apply_rotation(&mut target_global, &current, parent_transform.as_ref());

        self.connection.next(&mut current, target_global.position, dt);

        match &self.deformation
        {
            Deformation::Rigid => (),
            Deformation::Stretch(deformation) =>
            {
                let local_velocity = self.physical().map(|x| *x.velocity())
                    .unwrap_or_default();

                let global_velocity = physical.map(|x| *x.velocity())
                    .unwrap_or_default();

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
            Connection::Ignore
            | Connection::Rigid
            | Connection::Constant{..}
            | Connection::Timed{..} => (),
            Connection::Limit{limit} =>
            {
                *limit = new_limit;
            },
            Connection::EaseOut{limit, ..} =>
            {
                *limit = Some(new_limit);
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

    fn target_global_unrotated(
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

    pub fn target_global(
        &self,
        parent: Option<&Transform>
    ) -> Transform
    {
        let mut target = self.target_global_unrotated(parent);

        let current = target.clone();
        self.apply_rotation(&mut target, &current, parent);

        target
    }

    fn apply_rotation(
        &self,
        target: &mut Transform,
        current: &Transform,
        parent_transform: Option<&Transform>
    )
    {
        let rotation = NRotation::from_axis_angle(
            &Unit::new_normalize(Vector3::z()),
            current.rotation + self.origin_rotation
        );

        if !self.inherit_rotation
        {
            return;
        }

        if let Some(parent) = parent_transform
        {
            let scaled_origin = self.origin.component_mul(&parent.scale);
            let offset_position =
                self.target_local.position.component_mul(&parent.scale) - scaled_origin;

            target.position = rotation * offset_position + parent.position + scaled_origin;
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

    pub fn set_origin_rotation(&mut self, rotation: f32)
    {
        self.origin_rotation = rotation;
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
