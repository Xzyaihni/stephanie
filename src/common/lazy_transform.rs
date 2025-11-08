use std::f32;

use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::{
    short_rotation,
    rotate_point_z_3d,
    lerp,
    EaseOut,
    Entity,
    Physical,
    ColliderType,
    watcher::Lifetime
};


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpringConnection
{
    pub physical: Physical,
    pub limit: LimitMode,
    pub strength: f32
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EaseOutRotation
{
    pub decay: f32,
    pub speed_significant: f32,
    pub momentum: f32
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstantRotation
{
    pub speed: f32,
    pub momentum: f32
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LimitMode
{
    Normal(f32),
    Manhattan(Vector3<f32>)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Connection
{
    Ignore,
    Rigid,
    Limit{mode: LimitMode},
    Timed(TimedConnection),
    Constant{speed: f32},
    EaseOut{decay: f32, limit: Option<LimitMode>},
    EaseIn(EaseInInfo),
    Spring(SpringConnection)
}

impl Connection
{
    pub fn simple_next_2d(
        &mut self,
        current: &mut Vector2<f32>,
        target: Vector2<f32>,
        dt: f32
    )
    {
        let target = Vector3::new(target.x, target.y, 0.0);
        let mut position = Vector3::new(current.x, current.y, 0.0);
        self.simple_next(&mut position, target, dt);

        *current = position.xy();
    }

    pub fn simple_next(
        &mut self,
        current: &mut Vector3<f32>,
        target: Vector3<f32>,
        dt: f32
    )
    {
        match self
        {
            Connection::Ignore => (),
            Connection::Rigid =>
            {
                *current = target;
            },
            Connection::Timed(TimedConnection{lifetime, ref mut start}) =>
            {
                if start.is_none()
                {
                    *start = Some(*current);
                }

                let remaining = 1.0 - lifetime.fraction();

                *current = start.unwrap().zip_map(&target, |a, b|
                {
                    lerp(a, b, remaining)
                });

                lifetime.current -= dt;
            },
            Connection::Constant{speed} =>
            {
                let mut difference = target - *current;
                if difference.magnitude() > *speed { difference.set_magnitude(*speed); }

                *current += difference;
            },
            Connection::Limit{mode} =>
            {
                *current = LazyTransform::clamp_distance(
                    *mode,
                    target,
                    *current
                );
            },
            Connection::EaseOut{decay, limit} =>
            {
                *current = current.ease_out(target, *decay, dt);

                if let Some(limit) = limit
                {
                    *current = LazyTransform::clamp_distance(
                        *limit,
                        target,
                        *current
                    );
                }
            },
            Connection::EaseIn(info) =>
            {
                info.next(current, target, dt);
            },
            _ => ()
        }

        current.z = target.z;
    }

    pub fn next(
        &mut self,
        current: &mut Transform,
        target: Vector3<f32>,
        dt: f32
    )
    {
        match self
        {
            Connection::Spring(connection) =>
            {
                let distance = target - current.position;

                let spring_velocity = distance * connection.strength;

                connection.physical.add_velocity_raw(spring_velocity * dt);
                connection.physical.update(
                    current,
                    |physical, transform| ColliderType::Circle.inverse_inertia(physical, &transform.scale),
                    dt
                );

                connection.physical.apply(current);

                current.position = LazyTransform::clamp_distance(
                    connection.limit,
                    target,
                    current.position
                );
            },
            _ =>
            {
                self.simple_next(&mut current.position, target, dt)
            }
        }

        current.position.z = target.z;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Rotation
{
    Ignore,
    Instant,
    EaseOut(EaseOutRotationInfo),
    Constant(ConstantRotationInfo)
}

impl Rotation
{
    pub fn next(
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
pub struct EaseInInfo
{
    velocity: f32,
    strength: f32
}

impl PartialEq for EaseInInfo
{
    fn eq(&self, other: &Self) -> bool
    {
        self.strength.eq(&other.strength)
    }
}

impl EaseInInfo
{
    pub fn new(strength: f32) -> Self
    {
        Self{velocity: 0.0, strength}
    }

    pub fn next(&mut self, current: &mut Vector3<f32>, target: Vector3<f32>, dt: f32)
    {
        let difference = target - *current;

        self.velocity += self.strength * dt;
        *current += Vector3::repeat(self.velocity)
            .component_mul(&(difference.map(f32::signum)))
            .zip_map(&difference, |x, limit|
            {
                if limit < 0.0
                {
                    x.max(limit)
                } else
                {
                    x.min(limit)
                }
            });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringScaling
{
    velocity: Vector3<f32>,
    damping: f32,
    strength: f32
}

impl PartialEq for SpringScaling
{
    fn eq(&self, other: &Self) -> bool
    {
        self.damping == other.damping
            && self.strength == other.strength
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpringScalingInfo
{
    pub start_velocity: Vector2<f32>,
    pub damping: f32,
    pub strength: f32
}

impl From<SpringScalingInfo> for SpringScaling
{
    fn from(info: SpringScalingInfo) -> Self
    {
        Self{
            velocity: Vector3::new(info.start_velocity.x, info.start_velocity.y, 0.0),
            damping: info.damping,
            strength: info.strength
        }
    }
}

impl SpringScaling
{
    pub fn new(info: SpringScalingInfo) -> Self
    {
        info.into()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Scaling
{
    Ignore,
    Instant,
    EaseOut{decay: f32},
    Constant{speed: f32},
    EaseIn(EaseInInfo),
    Spring(SpringScaling)
}

impl Scaling
{
    pub fn next_2d(
        &mut self,
        current: &mut Vector2<f32>,
        target: Vector2<f32>,
        dt: f32
    )
    {
        let target = Vector3::new(target.x, target.y, 1.0);
        let mut scale = Vector3::new(current.x, current.y, 1.0);
        self.next(&mut scale, target, dt);

        *current = scale.xy();
    }

    pub fn next(
        &mut self,
        current: &mut Vector3<f32>,
        target: Vector3<f32>,
        dt: f32
    )
    {
        match self
        {
            Scaling::Ignore => (),
            Scaling::Instant =>
            {
                *current = target;
            },
            Scaling::Constant{speed} =>
            {
                let max_move = Vector3::repeat(*speed * dt);

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
            },
            Scaling::EaseIn(info) =>
            {
                info.next(current, target, dt);
            },
            Scaling::Spring(SpringScaling{velocity, damping, strength}) =>
            {
                let difference = (target - *current) * *strength;

                *velocity += difference * dt;

                *current += *velocity * dt;

                *velocity *= damping.powf(dt);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Deformation
{
    Rigid,
    Stretch(StretchDeformation)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FollowRotation
{
    pub parent: Entity,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FollowPosition
{
    pub parent: Entity,
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
        self.connection.next(current, self.target_end(current.rotation, parent_position), dt);
    }

    pub fn target_end(
        &self,
        rotation: f32,
        parent_position: Vector3<f32>
    ) -> Vector3<f32>
    {
        rotate_point_z_3d(self.offset, rotation) + parent_position
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
    pub combine_origin_rotation: bool,
    pub origin_rotation_interpolation: Option<f32>,
    pub origin_rotation: f32,
    pub origin: Vector3<f32>,
    pub unscaled_position: bool,
    pub inherit_position: bool,
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
            combine_origin_rotation: false,
            origin_rotation_interpolation: None,
            origin_rotation: 0.0,
            origin: Vector3::zeros(),
            unscaled_position: false,
            inherit_position: true,
            inherit_scale: true,
            inherit_rotation: true,
            transform: Transform::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct OriginRotationInterpolation
{
    pub decay: f32,
    pub current: f32
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LazyTransform
{
    pub target_local: Transform,
    combine_origin_rotation: bool,
    origin_rotation_interpolation: Option<OriginRotationInterpolation>,
    origin_rotation: f32,
    origin: Vector3<f32>,
    unscaled_position: bool,
    inherit_position: bool,
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
        let origin_rotation_interpolation = info.origin_rotation_interpolation.map(|decay|
        {
            OriginRotationInterpolation{decay, current: info.origin_rotation}
        });

        Self{
            target_local: info.transform,
            combine_origin_rotation: info.combine_origin_rotation,
            origin_rotation_interpolation,
            origin_rotation: info.origin_rotation,
            origin: info.origin,
            unscaled_position: info.unscaled_position,
            inherit_position: info.inherit_position,
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
        mut current: Transform,
        parent_transform: Option<Transform>,
        dt: f32
    ) -> Transform
    {
        let mut target_global = self.target_global_unrotated(parent_transform.as_ref());

        let pi2 = 2.0 * f32::consts::PI;
        current.rotation %= pi2;
        target_global.rotation %= pi2;

        if let Some(x) = self.origin_rotation_interpolation.as_mut()
        {
            x.current = x.current.ease_out(self.origin_rotation, x.decay, dt);
        }

        self.preapply_rotation(&mut target_global);

        self.scaling.next(&mut current.scale, target_global.scale, dt);
        self.rotation.next(&mut current.rotation, target_global.rotation, dt);

        self.apply_rotation(&mut target_global, &current, parent_transform.as_ref());

        let previous_position = current.position;
        self.connection.next(&mut current, target_global.position, dt);

        match &self.deformation
        {
            Deformation::Rigid => (),
            Deformation::Stretch(deformation) =>
            {
                let velocity = (current.position - previous_position) / dt;

                current.stretch = deformation.stretch(current.rotation, velocity);
            }
        }

        current
    }

    pub fn set_connection_limit(&mut self, new_limit: LimitMode)
    {
        match &mut self.connection
        {
            Connection::Ignore
            | Connection::Rigid
            | Connection::Constant{..}
            | Connection::Timed{..}
            | Connection::EaseIn(..) => (),
            Connection::Limit{mode} =>
            {
                *mode = new_limit;
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

    fn scaled_position(&self, parent_scale: &Vector3<f32>) -> Vector3<f32>
    {
        if self.unscaled_position
        {
            self.target_local.position
        } else
        {
            self.target_local.position.component_mul(parent_scale)
        }
    }

    pub fn combine(&self, parent: &Transform) -> Transform
    {
        let mut transform = self.target_local.clone();

        transform.position = self.scaled_position(&parent.scale);

        if self.inherit_position
        {
            transform.position += parent.position;
        }

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

        self.preapply_rotation(&mut target);

        let current = target.clone();
        self.apply_rotation(&mut target, &current, parent);

        target
    }

    fn preapply_rotation(&self, target: &mut Transform)
    {
        let origin_rotation = if let Some(x) = self.origin_rotation_interpolation.as_ref()
        {
            x.current
        } else
        {
            self.origin_rotation
        };

        if self.combine_origin_rotation
        {
            target.rotation += origin_rotation;
        }
    }

    fn apply_rotation(
        &self,
        target: &mut Transform,
        current: &Transform,
        parent_transform: Option<&Transform>
    )
    {
        if !self.inherit_rotation
        {
            return;
        }

        let origin_rotation = if let Some(x) = self.origin_rotation_interpolation
        {
            x.current
        } else
        {
            self.origin_rotation
        };

        if let Some(parent) = parent_transform
        {
            let rotation = current.rotation + origin_rotation;

            let scaled_origin = rotate_point_z_3d(self.origin.component_mul(&parent.scale), self.target_local.rotation);

            let scaled_position = self.scaled_position(&parent.scale);
            let offset_position = scaled_position - scaled_origin;

            target.position = rotate_point_z_3d(offset_position, rotation) + scaled_origin;

            if self.inherit_position
            {
                target.position += parent.position;
            }
        } else
        {
            if self.origin != Vector3::zeros()
            {
                let scaled_origin = rotate_point_z_3d(self.origin.component_mul(&self.target_local.scale), self.target_local.rotation);

                let origin_position = self.target_local.position + scaled_origin;

                let around_origin = rotate_point_z_3d(self.target_local.position - origin_position, origin_rotation);
                target.position = around_origin + origin_position;
            }
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

    fn clamp_distance(
        mode: LimitMode,
        target: Vector3<f32>,
        current: Vector3<f32>
    ) -> Vector3<f32>
    {
        let distance = target - current;

        match mode
        {
            LimitMode::Normal(limit) =>
            {
                if distance.magnitude() < limit
                {
                    return current;
                }

                let limited_position = distance.normalize() * limit;

                target - limited_position
            },
            LimitMode::Manhattan(limit) =>
            {
                if limit.iter().any(|x| *x < 0.0)
                {
                    return current;
                }

                target - distance.zip_map(&limit, |x, limit: f32| x.clamp(-limit, limit))
            }
        }
    }
}
