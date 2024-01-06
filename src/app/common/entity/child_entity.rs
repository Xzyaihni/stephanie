use serde::{Serialize, Deserialize};

use nalgebra::Vector2;

use yanyaengine::{
    Transform,
    TransformContainer,
	OnTransformCallback
};

use crate::entity_forward;


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
	limit: f32,
	strength: f32
}

impl SpringConnection
{
	#[allow(dead_code)]
	pub fn new(limit: f32, strength: f32) -> Self
	{
		Self{limit, strength}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StretchDeformation
{
	animation: ValueAnimation,
	limit: f32,
	strength: f32
}

impl StretchDeformation
{
	#[allow(dead_code)]
	pub fn new(animation: ValueAnimation, limit: f32, strength: f32) -> Self
	{
		Self{animation, limit, strength}
	}

	pub fn stretch(&mut self, velocity: Vector3<f32>) -> (f32, Vector2<f32>)
	{
		let amount = self.animation.apply(velocity.magnitude() * self.strength);
		let stretch = (1.0 + amount).max(self.limit);

		let angle = velocity.y.atan2(-velocity.x);

		(angle, Vector2::new(stretch, 1.0 / stretch))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChildConnection
{
	Rigid,
	Spring(SpringConnection)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChildDeformation
{
	Rigid,
	Stretch(StretchDeformation)
}

pub struct ChildEntityRef<'a, P>
{
    parent: &'a P,
    entity: &'a mut ChildEntity
}

impl<'a, P> ChildEntityRef<'a, P>
where
    P: TransformContainer
{
	pub fn set_origin(&mut self, origin: Vector3<f32>)
	{
		self.entity.origin = origin.component_mul(self.parent.scale());
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildEntity
{
	connection: ChildConnection,
	deformation: ChildDeformation,
	origin: Vector3<f32>,
	entity: Entity,
	z_level: i32
}

impl ChildEntity
{
	pub fn new(
		connection: ChildConnection,
		deformation: ChildDeformation,
		entity: Entity,
		z_level: i32
	) -> Self
	{
		let origin = Vector3::zeros();

		Self{connection, deformation, origin, entity, z_level}
	}

    pub fn with_parent<'a, P>(&'a mut self, parent: &'a P) -> ChildEntityRef<'a, P>
    where
        P: TransformContainer
    {
        ChildEntityRef{parent, entity: self}
    }

    // positive = above parent
    // negative = below parent
	pub fn z_level(&self) -> i32
	{
		self.z_level
	}

	pub fn origin(&self) -> Vector3<f32>
	{
		self.origin
	}

	pub fn update(&mut self, parent_physical: &Physical, dt: f32)
	{
		let requires_physics = match &mut self.connection
		{
			ChildConnection::Rigid =>
            {
                self.entity.physical = parent_physical.clone();

                false
            },
			ChildConnection::Spring(connection) =>
			{
                let transform = &self.entity.physical.transform;
                let parent_transform = &parent_physical.transform;

                let distance = parent_transform.position - transform.position;

                let spring_force = distance * connection.strength;

                self.entity.physical.force += spring_force;

                true
			}
		};

		match &mut self.deformation
		{
			ChildDeformation::Rigid => (),
			ChildDeformation::Stretch(deformation) =>
			{
				let stretch = deformation.stretch(self.entity.physical.velocity);

				self.entity.set_stretch(stretch);
			}
		}

        if requires_physics
        {
            self.entity.physics_update(dt);

            self.entity.physical.transform.position.z = parent_physical.transform.position.z;
        }
	}
}

entity_forward!{ChildEntity, entity}
