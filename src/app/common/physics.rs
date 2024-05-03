use nalgebra::Vector3;

use serde::{Serialize, Deserialize};

use yanyaengine::{Transform, TransformContainer, OnTransformCallback};


pub const GRAVITY: Vector3<f32> = Vector3::new(0.0, 0.0, -9.81);

#[derive(Clone)]
pub struct PhysicalProperties
{
	pub transform: Transform,
    pub mass: f32,
    pub friction: f32,
    pub floating: bool
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Physical
{
	pub transform: Transform,
    pub mass: f32,
	pub friction: f32,
    pub floating: bool,
    pub grounded: bool,
	pub velocity: Vector3<f32>,
	pub force: Vector3<f32>,
}

impl From<PhysicalProperties> for Physical
{
    fn from(value: PhysicalProperties) -> Self
    {
        Self{
            transform: value.transform,
            mass: value.mass,
            friction: value.friction,
            floating: value.floating,
            grounded: false,
            velocity: Vector3::zeros(),
            force: Vector3::zeros()
        }
    }
}

impl OnTransformCallback for Physical {}

impl TransformContainer for Physical
{
	fn transform_ref(&self) -> &Transform
	{
		&self.transform
	}

	fn transform_mut(&mut self) -> &mut Transform
	{
		&mut self.transform
	}
}

impl PhysicsEntity for Physical
{
	fn physical_ref(&self) -> &Physical
    {
        self
    }

	fn physical_mut(&mut self) -> &mut Physical
    {
        self
    }
}

impl Physical
{
    pub fn physics_update(&mut self, dt: f32)
    {
        self.velocity += (self.force * dt) / self.mass;

        // move this up after i add collisions
        self.force += self.mass * GRAVITY;

        if self.grounded
        {
            let normal_impulse = (-self.force.z * dt).max(0.0);

            self.apply_friction(normal_impulse);
        }

        self.transform.position += self.velocity * dt;

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

    fn add_impulse(&mut self, impulse: Vector3<f32>)
    {
        self.velocity += self.impulse_to_velocity(impulse);
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
}

pub trait PhysicsEntity: TransformContainer
{
	fn physical_ref(&self) -> &Physical;
	fn physical_mut(&mut self) -> &mut Physical;

    fn set_velocity(&mut self, velocity: Vector3<f32>)
    {
        self.physical_mut().velocity = velocity;
    }

    fn add_force(&mut self, force: Vector3<f32>)
    {
        self.physical_mut().force += force;
    }

    fn sub_impulse(&mut self, impulse: Vector3<f32>)
    {
        self.physical_mut().sub_impulse(impulse);
    }

    fn add_impulse(&mut self, impulse: Vector3<f32>)
    {
        self.physical_mut().add_impulse(impulse);
    }

    fn damp_velocity(&mut self, damping: f32, dt: f32)
    {
        self.physical_mut().velocity *= damping.powf(dt);
    }

	fn physics_update(&mut self, dt: f32)
    {
        self.physical_mut().physics_update(dt);
    }

	fn sync_transform(&mut self, other: Transform)
	{
		let physical = self.physical_mut();

        physical.transform = other;

		self.transform_callback(self.transform_clone());
	}
}
