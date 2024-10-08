use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use yanyaengine::Transform;

use crate::{
    debug_config::*,
    common::{
        cross_3d,
        ENTITY_SCALE
    }
};


pub const GRAVITY: Vector3<f32> = Vector3::new(0.0, 0.0, -9.81 * ENTITY_SCALE);
pub const MAX_VELOCITY: f32 = 10.0;
const SLEEP_THRESHOLD: f32 = 0.03;
const MOVEMENT_BIAS: f32 = 0.8;

const SLEEP_MOVEMENT_MAX: f32 = SLEEP_THRESHOLD * 16.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PhysicalFixed
{
    pub rotation: bool
}

impl Default for PhysicalFixed
{
    fn default() -> Self
    {
        Self{
            rotation: false
        }
    }
}

#[derive(Clone)]
pub struct PhysicalProperties
{
    pub inverse_mass: f32,
    pub restitution: f32,
    pub damping: f32,
    pub angular_damping: f32,
    pub static_friction: f32,
    pub dynamic_friction: f32,
    pub can_sleep: bool,
    pub floating: bool,
    pub fixed: PhysicalFixed,
    pub target_non_lazy: bool,
    pub move_z: bool
}

impl Default for PhysicalProperties
{
    fn default() -> Self
    {
        Self{
            inverse_mass: 1.0,
            restitution: 0.3,
            static_friction: 0.5,
            dynamic_friction: 0.4,
            damping: 0.9,
            angular_damping: 0.4,
            can_sleep: true,
            floating: false,
            fixed: PhysicalFixed::default(),
            target_non_lazy: false,
            move_z: true
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Physical
{
    pub inverse_mass: f32,
    pub restitution: f32,
    pub static_friction: f32,
    pub dynamic_friction: f32,
    pub fixed: PhysicalFixed,
    pub target_non_lazy: bool,
    pub move_z: bool,
    floating: bool,
    can_sleep: bool,
    sleeping: bool,
    sleep_movement: f32,
    angular_damping: f32,
    torque: f32,
    angular_velocity: f32,
    angular_acceleration: f32,
    damping: f32,
    force: Vector3<f32>,
    velocity: Vector3<f32>,
    acceleration: Vector3<f32>,
    last_acceleration: Vector3<f32>
}

impl From<PhysicalProperties> for Physical
{
    fn from(props: PhysicalProperties) -> Self
    {
        Self{
            inverse_mass: props.inverse_mass,
            restitution: props.restitution,
            static_friction: props.static_friction,
            dynamic_friction: props.dynamic_friction,
            floating: props.floating,
            fixed: props.fixed,
            target_non_lazy: props.target_non_lazy,
            move_z: props.move_z,
            can_sleep: props.can_sleep,
            sleeping: false,
            sleep_movement: SLEEP_MOVEMENT_MAX,
            angular_damping: props.angular_damping,
            torque: 0.0,
            angular_velocity: 0.0,
            angular_acceleration: 0.0,
            damping: props.damping,
            force: Vector3::zeros(),
            velocity: Vector3::zeros(),
            acceleration: Vector3::zeros(),
            last_acceleration: Vector3::zeros()
        }
    }
}

impl Physical
{
    pub fn as_properties(&self) -> PhysicalProperties
    {
        PhysicalProperties{
            inverse_mass: self.inverse_mass,
            restitution: self.restitution,
            damping: self.damping,
            angular_damping: self.angular_damping,
            static_friction: self.static_friction,
            dynamic_friction: self.dynamic_friction,
            can_sleep: self.can_sleep,
            floating: self.floating,
            fixed: self.fixed,
            target_non_lazy: self.target_non_lazy,
            move_z: self.move_z
        }
    }

    pub fn update(
        &mut self,
        transform: &mut Transform,
        inverse_inertia: impl Fn(&Physical, &Transform) -> f32,
        dt: f32
    )
    {
        if self.sleeping
        {
            return;
        }

        if !self.floating && DebugConfig::is_disabled(DebugTool::NoGravity)
        {
            self.acceleration = GRAVITY;
        }

        transform.position += self.velocity * dt;

        if !self.fixed.rotation
        {
            transform.rotation += self.angular_velocity * dt;
        }

        self.last_acceleration = self.acceleration + self.force * self.inverse_mass;

        self.velocity += self.last_acceleration * dt;
        self.velocity *= self.damping.powf(dt);

        if self.velocity.magnitude() > MAX_VELOCITY
        {
            self.velocity.set_magnitude(MAX_VELOCITY);
        }

        self.force = Vector3::zeros();

        if self.inverse_mass != 0.0
        {
            let inverse_inertia = inverse_inertia(self, transform);
            let angular_acceleration = self.angular_acceleration + self.torque * inverse_inertia;

            self.angular_velocity += angular_acceleration * dt;
            self.angular_velocity *= self.angular_damping.powf(dt);

            self.torque = 0.0;
        }

        if self.can_sleep
        {
            self.update_sleep_movement(dt);
        }
    }

    pub fn floating(&self) -> bool
    {
        self.floating
    }

    pub fn set_floating(&mut self, state: bool)
    {
        self.floating = state;

        if state
        {
            self.acceleration = Vector3::zeros();
        }
    }

    pub fn update_sleep_movement(&mut self, dt: f32)
    {
        let new_movement = (self.velocity.map(|x| x.powi(2)).sum() + self.angular_velocity).abs();

        let bias = MOVEMENT_BIAS.powf(dt);
        self.sleep_movement = bias * self.sleep_movement + (1.0 - bias) * new_movement;

        self.sleep_movement = self.sleep_movement.min(SLEEP_MOVEMENT_MAX);

        if self.sleep_movement < SLEEP_THRESHOLD
        {
            self.set_sleeping(true);
        }
    }

    pub fn set_sleeping(&mut self, state: bool)
    {
        if self.sleeping == state
        {
            return;
        }

        self.sleeping = state;
        if state
        {
            self.velocity = Vector3::zeros();
            self.angular_velocity = 0.0;
        } else
        {
            self.sleep_movement = SLEEP_THRESHOLD * 2.0;
        }
    }

    pub fn last_acceleration(&self) -> &Vector3<f32>
    {
        &self.last_acceleration
    }

    pub fn sleeping(&self) -> bool
    {
        self.sleeping
    }

    pub fn set_acceleration(&mut self, acceleration: Vector3<f32>)
    {
        self.acceleration = acceleration;
    }

    pub fn velocity(&self) -> &Vector3<f32>
    {
        &self.velocity
    }

    pub fn angular_velocity(&self) -> f32
    {
        self.angular_velocity
    }

    pub fn set_velocity_raw(&mut self, velocity: Vector3<f32>)
    {
        self.velocity = velocity;
    }

    pub fn velocity_as_force(&self, velocity: Vector3<f32>, dt: f32) -> Vector3<f32>
    {
        velocity / dt / self.inverse_mass
    }

    pub fn add_velocity(&mut self, velocity: Vector3<f32>, dt: f32)
    {
        self.add_force(self.velocity_as_force(velocity, dt));
    }

    pub fn add_velocity_raw(&mut self, velocity: Vector3<f32>)
    {
        self.velocity += velocity;
    }

    pub fn add_angular_velocity_raw(&mut self, velocity: f32)
    {
        self.angular_velocity += velocity;
    }

    pub fn add_force(&mut self, force: Vector3<f32>)
    {
        self.force += force;

        self.set_sleeping(false);
    }

    pub fn add_torque(&mut self, torque: f32)
    {
        self.torque += torque;
    }

    pub fn add_force_at_point(&mut self, force: Vector3<f32>, point: Vector3<f32>)
    {
        self.add_force(force);

        self.add_torque(cross_3d(point, force).z);
    }
}
