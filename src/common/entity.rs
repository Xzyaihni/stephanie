use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3};

use transform::{
	Transform,
	OnTransformCallback,
	TransformContainer
};

use crate::{
	client::DrawableEntity,
	common::{
		ChildContainer,
		physics::PhysicsEntity
	}
};

pub mod transform;


#[derive(Debug, Clone)]
pub struct EntityProperties
{
	pub transform: Transform,
	pub texture: String,
	pub damp_factor: f32
}

impl Default for EntityProperties
{
	fn default() -> Self
	{
		let mut transform = Transform::new();
		transform.scale = Vector3::new(0.1, 0.1, 1.0);

		let texture = String::new();

		Self{transform, texture, damp_factor: 0.5}
	}
}

fn limit_distance(limit: f32, distance: f32) -> f32
{
	(1.0 - (limit / distance)).max(0.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpringConnection
{
	limit: f32,
	damping: f32,
	strength: f32
}

impl SpringConnection
{
	pub fn new(limit: f32, damping: f32, strength: f32) -> Self
	{
		Self{limit, damping, strength}
	}

	pub fn springed(
		&mut self,
		velocity: &mut Vector3<f32>,
		position: Vector3<f32>,
		translation: Vector3<f32>,
		dt: f32
	) -> Vector3<f32>
	{
		let translation = translation + ChildEntity::damp_velocity(
			velocity,
			self.damping,
			dt
		);

		let spring_velocity = -position * self.strength;

		*velocity += spring_velocity;

		let new_position = position + translation;

		if self.limit >= new_position.magnitude()
		{
			new_position
		} else
		{
			new_position.normalize() * self.limit
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayedConnection
{
	limit: f32,
	strength: f32
}

impl DelayedConnection
{
	pub fn new(limit: f32, strength: f32) -> Self
	{
		Self{limit, strength}
	}

	pub fn translate_amount(&mut self, distance: f32, dt: f32) -> f32
	{
		(self.strength * dt).max(limit_distance(self.limit, distance)).min(1.0)
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StretchDeformation
{
	limit: f32,
	strength: f32
}

impl StretchDeformation
{
	pub fn new(limit: f32, strength: f32) -> Self
	{
		Self{limit, strength}
	}

	pub fn stretched(&mut self, velocity: Vector3<f32>) -> (f32, Vector2<f32>)
	{
		let stretch = (1.0 + velocity.magnitude() * self.strength).min(self.limit);

		let angle = velocity.y.atan2(-velocity.x);

		(angle, Vector2::new(stretch, 1.0 / stretch))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChildConnection
{
	Rigid,
	Spring(SpringConnection),
	Delayed(DelayedConnection)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChildDeformation
{
	Rigid,
	Stretch(StretchDeformation)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildEntity
{
	connection: ChildConnection,
	deformation: ChildDeformation,
	origin: Vector3<f32>,
	transform: Transform,
	entity: Entity
}

impl ChildEntity
{
	pub fn new(connection: ChildConnection, deformation: ChildDeformation, entity: Entity) -> Self
	{
		let origin = Vector3::zeros();
		let transform = entity.transform_clone();

		Self{connection, deformation, origin, transform, entity}
	}

	pub fn origin(&self) -> Vector3<f32>
	{
		self.origin
	}

	pub fn set_origin(&mut self, owner: &impl TransformContainer, origin: Vector3<f32>)
	{
		self.origin = origin.component_mul(owner.scale());
	}

	pub fn relative_transform(&mut self, transform: Transform)
	{
		let world_transform = self.entity.transform_mut();
		let this_transform = &self.transform;

		world_transform.position = this_transform.position.component_mul(&transform.scale)
			+ transform.position;

		world_transform.scale = this_transform.scale.component_mul(&transform.scale);
		world_transform.rotation = this_transform.rotation + transform.rotation;
		world_transform.rotation_axis = transform.rotation_axis;
	}
}

impl OnTransformCallback for ChildEntity {}

impl TransformContainer for ChildEntity
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

impl DrawableEntity for ChildEntity
{
	fn texture(&self) -> &str
	{
		self.entity.texture()
	}
}

impl PhysicsEntity for ChildEntity
{
	fn entity_ref(&self) -> &Entity
	{
		&self.entity
	}

	fn entity_mut(&mut self) -> &mut Entity
	{
		&mut self.entity
	}

	fn update(&mut self, dt: f32)
	{
		let distance = self.transform.position.magnitude();

		let translation = Self::damp_velocity(
			&mut self.entity.velocity,
			self.entity.damp_factor,
			dt
		);

		match &mut self.connection
		{
			ChildConnection::Rigid => (),
			ChildConnection::Spring(connection) =>
			{
				let position = connection.springed(
					&mut self.entity.velocity,
					self.transform.position,
					translation,
					dt
				);

				self.set_position(position);
			},
			ChildConnection::Delayed(connection) =>
			{
				let amount = connection.translate_amount(distance, dt);

				self.translate_to(Vector3::zeros(), amount);
			}
		}

		match &mut self.deformation
		{
			ChildDeformation::Rigid => (),
			ChildDeformation::Stretch(deformation) =>
			{
				let stretch = deformation.stretched(self.entity.velocity);

				self.entity.set_stretch(stretch);
			}
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity
{
	damp_factor: f32,
	transform: Transform,
	texture: String,
	pub velocity: Vector3<f32>,
	children: Vec<ChildEntity>
}

impl Entity
{
	pub fn new(properties: EntityProperties) -> Self
	{
		let EntityProperties{damp_factor, transform, texture} = properties;

		let velocity = Vector3::zeros();

		let children = Vec::new();

		Self{damp_factor, transform, texture, velocity, children}
	}
}

impl OnTransformCallback for Entity
{
	fn transform_callback(&mut self, transform: Transform)
	{
		self.children.iter_mut().for_each(|child| child.relative_transform(transform.clone()));
	}
}

impl TransformContainer for Entity
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

impl ChildContainer for Entity
{
	fn children_ref(&self) -> &[ChildEntity]
	{
		&self.children
	}

	fn children_mut(&mut self) -> &mut Vec<ChildEntity>
	{
		&mut self.children
	}
}

impl PhysicsEntity for Entity
{
	fn entity_ref(&self) -> &Entity
	{
		self
	}

	fn entity_mut(&mut self) -> &mut Entity
	{
		self
	}

	fn update(&mut self, dt: f32)
	{
		let translation = Self::damp_velocity(&mut self.velocity, self.damp_factor, dt);
		self.translate(translation);

		self.children.iter_mut().for_each(|child|
		{
			child.update(dt);
		});

		self.transform_callback(self.transform.clone());
	}

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.entity_mut().velocity += velocity;

		self.children.iter_mut().for_each(|child|
		{
			child.velocity_add(velocity);
		});
	}
}

impl DrawableEntity for Entity
{
	fn texture(&self) -> &str
	{
		&self.texture
	}
}