use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use yanyaengine::Transform;

use crate::common::ENTITY_SCALE;


pub const GRAVITY: f32 = -9.81 * ENTITY_SCALE;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PhysicalFixed
{
    pub position: bool
}

impl Default for PhysicalFixed
{
    fn default() -> Self
    {
        Self{
            position: false
        }
    }
}

#[derive(Clone)]
pub struct PhysicalProperties
{
    pub mass: f32,
    pub friction: f32,
    pub floating: bool,
    pub fixed: PhysicalFixed
}

impl Default for PhysicalProperties
{
    fn default() -> Self
    {
        Self{
            mass: 1.0,
            friction: 0.5,
            floating: false,
            fixed: PhysicalFixed::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Physical
{
    pub mass: f32,
    pub friction: f32,
    pub floating: bool,
    pub fixed: PhysicalFixed,
    pub grounded: bool,
    pub velocity: Vector3<f32>,
    pub force: Vector3<f32>
}

impl From<PhysicalProperties> for Physical
{
    fn from(value: PhysicalProperties) -> Self
    {
        Self{
            mass: value.mass,
            friction: value.friction,
            floating: value.floating,
            fixed: value.fixed,
            grounded: false,
            velocity: Vector3::zeros(),
            force: Vector3::zeros()
        }
    }
}

impl Physical
{
    pub fn as_properties(&self) -> PhysicalProperties
    {
        PhysicalProperties{
            mass: self.mass,
            friction: self.friction,
            floating: self.floating,
            fixed: self.fixed
        }
    }

    pub fn physics_update(
        &mut self,
        transform: &mut Transform,
        dt: f32
    )
    {
        if !self.floating
        {
            self.force.z += self.mass * GRAVITY;
            self.grounded = true;
        }

        self.velocity += (self.force * dt) / self.mass;

        if self.grounded
        {
            let normal_impulse = (-self.force.z * dt).max(0.0);

            self.apply_friction(normal_impulse);
        }

        if !self.fixed.position
        {
            transform.position += self.velocity * dt;
        }

        self.force = Vector3::zeros();
    }

    fn impulse_to_velocity(&self, impulse: Vector3<f32>) -> Vector3<f32>
    {
        impulse / self.mass
    }

    fn sub_impulse(&mut self, impulse: Vector3<f32>)
    {
        self.velocity -= self.impulse_to_velocity(impulse);
    }

    #[allow(dead_code)]
    fn add_impulse(&mut self, impulse: Vector3<f32>)
    {
        self.velocity += self.impulse_to_velocity(impulse);
    }

    pub fn invert_velocity(&mut self)
    {
        self.velocity = -self.velocity;
    }

    // i have no clue if normal impulse is a real thing lmao
    fn apply_friction(&mut self, normal_impulse: f32)
    {
        let mut movement_velocity = self.velocity;
        movement_velocity.z = 0.0;

        if let Some(movement_direction) = movement_velocity.try_normalize(f32::EPSILON * 2.0)
        {
            let static_friction = self.friction * 1.25;
            let static_friction_impulse = normal_impulse * static_friction;

            let tangent_force = movement_velocity.magnitude() * self.mass;

            if tangent_force < static_friction_impulse
            {
                self.sub_impulse(movement_direction * tangent_force);
            } else
            {
                let kinetic_friction_impulse = normal_impulse * self.friction;

                self.sub_impulse(movement_direction * kinetic_friction_impulse);
            }
        } else
        {
            self.velocity.x = 0.0;
            self.velocity.y = 0.0;
        }
    }

    pub fn damp_velocity(&mut self, damping: f32, dt: f32)
    {
        self.velocity *= damping.powf(dt);
    }
}
