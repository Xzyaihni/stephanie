use std::mem;

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
pub const SLEEPING_VELOCITY: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroundedState
{
    Air,
    WasGrounded,
    Grounded
}

#[derive(Clone)]
pub struct PhysicalProperties
{
    pub inverse_mass: f32,
    pub restitution: f32,
    pub damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
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
            damping: 0.99,
            angular_damping: 0.99,
            friction: 2.0,
            floating: false,
            fixed: PhysicalFixed::default(),
            target_non_lazy: false,
            move_z: true
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Physical
{
    pub inverse_mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub fixed: PhysicalFixed,
    pub target_non_lazy: bool,
    pub move_z: bool,
    pub next_position: Vector3<f32>,
    floating: bool,
    grounded: GroundedState,
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
            friction: props.friction,
            restitution: props.restitution,
            floating: props.floating,
            grounded: GroundedState::Air,
            fixed: props.fixed,
            target_non_lazy: props.target_non_lazy,
            move_z: props.move_z,
            angular_damping: props.angular_damping,
            torque: 0.0,
            angular_velocity: 0.0,
            angular_acceleration: 0.0,
            damping: props.damping,
            force: Vector3::zeros(),
            velocity: Vector3::zeros(),
            acceleration: Vector3::zeros(),
            last_acceleration: Vector3::zeros(),
            next_position: Vector3::zeros()
        }
    }
}

impl Physical
{
    pub fn as_properties(&self) -> PhysicalProperties
    {
        PhysicalProperties{
            inverse_mass: self.inverse_mass,
            friction: self.friction,
            restitution: self.restitution,
            damping: self.damping,
            angular_damping: self.angular_damping,
            floating: self.floating,
            fixed: self.fixed,
            target_non_lazy: self.target_non_lazy,
            move_z: self.move_z
        }
    }

    pub fn apply(&mut self, transform: &mut Transform)
    {
        transform.position = self.next_position;
    }

    pub fn update(
        &mut self,
        transform: &mut Transform,
        inverse_inertia: impl Fn(&Physical, &Transform) -> f32,
        dt: f32
    )
    {
        if !self.floating && self.move_z && DebugConfig::is_disabled(DebugTool::NoGravity)
        {
            self.acceleration = GRAVITY;
        }

        self.next_position = transform.position + self.velocity * dt;

        if !self.fixed.rotation
        {
            transform.rotation += self.angular_velocity * dt;
        }

        self.last_acceleration = self.acceleration + self.force * self.inverse_mass;

        self.velocity += self.last_acceleration * dt;

        if !self.floating && self.is_grounded()
        {
            let velocity_2d_magnitude = self.velocity.xy().magnitude();

            if velocity_2d_magnitude > 0.0001
            {
                let velocity_direction = self.velocity.xy() / velocity_2d_magnitude;

                let mut change_amount = self.friction * dt;
                let change_magnitude = change_amount.abs();

                if change_magnitude > velocity_2d_magnitude
                {
                    change_amount *= velocity_2d_magnitude / change_magnitude;
                }

                let change = velocity_direction * change_amount;

                self.velocity.x -= change.x;
                self.velocity.y -= change.y;
            } else
            {
                self.velocity.x = 0.0;
                self.velocity.y = 0.0;
            }
        }

        {
            let damping = self.damping.powf(dt);

            self.velocity.x *= damping;
            self.velocity.y *= damping;
        }

        if self.velocity.magnitude() > MAX_VELOCITY
        {
            self.velocity.set_magnitude(MAX_VELOCITY);
        }

        self.force = Vector3::zeros();

        if self.inverse_mass != 0.0 && !self.fixed.rotation
        {
            let inverse_inertia = inverse_inertia(self, transform);
            let angular_acceleration = self.angular_acceleration + self.torque * inverse_inertia;

            self.angular_velocity += angular_acceleration * dt;

            if !self.floating && self.is_grounded()
            {
                let max_scale = transform.scale.xy().max();

                if max_scale.abs() <= 0.0001
                {
                    return;
                }

                let mut change = (self.friction / max_scale) * self.angular_velocity.signum() * dt;

                let angular_magnitude = self.angular_velocity.abs();

                if change.abs() > angular_magnitude
                {
                    change *= angular_magnitude / change.abs();
                }

                self.angular_velocity -= change;
            }

            self.angular_velocity *= self.angular_damping.powf(dt);

            self.torque = 0.0;
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

    pub fn set_grounded(&mut self, state: bool)
    {
        if state
        {
            self.grounded = GroundedState::Grounded;
        } else
        {
            self.grounded = match self.grounded
            {
                GroundedState::Air
                | GroundedState::WasGrounded => GroundedState::Air,
                GroundedState::Grounded => GroundedState::WasGrounded
            };
        }
    }

    pub fn is_grounded(&self) -> bool
    {
        self.grounded != GroundedState::Air
    }

    pub fn last_acceleration(&self) -> &Vector3<f32>
    {
        &self.last_acceleration
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

    pub fn remove_velocity_axis(&mut self, axis: usize) -> f32
    {
        mem::take(&mut self.velocity[axis])
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
