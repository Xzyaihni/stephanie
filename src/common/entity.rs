use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use transform::{
	direction,
	distance,
	normalize,
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
		parent_position: Vector3<f32>,
		translation: Vector3<f32>
	) -> Vector3<f32>
	{
		let to_direction = direction(position, parent_position);

		let spring_velocity = to_direction * self.strength;

		*velocity += spring_velocity;

		let new_position = position + translation;

		if self.limit >= distance(new_position, parent_position)
		{
			new_position
		} else
		{
			let from_direction = direction(parent_position, new_position);

			parent_position + normalize(from_direction) * self.limit
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
	Stretch(f32)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildEntity
{
	connection: ChildConnection,
	deformation: ChildDeformation,
	scale: Vector3<f32>,
	entity: Entity
}

impl ChildEntity
{
	pub fn new(connection: ChildConnection, deformation: ChildDeformation, entity: Entity) -> Self
	{
		let scale = *entity.scale();

		Self{connection, deformation, scale, entity}
	}

	pub fn update(&mut self, parent_transform: Transform, dt: f32)
	{
		let distance = self.entity.distance(parent_transform.position);

		match &mut self.connection
		{
			ChildConnection::Rigid =>
			{
				self.entity.set_position(parent_transform.position);
			},
			ChildConnection::Spring(connection) =>
			{
				let translation = Self::damp_velocity(
					&mut self.entity.velocity,
					connection.damping,
					dt
				);

				let position = self.entity.transform_ref().position;
				let position = connection.springed(
					&mut self.entity.velocity,
					position,
					parent_transform.position,
					translation
				);

				self.entity.set_position(position);
			},
			ChildConnection::Delayed(connection) =>
			{
				let amount = connection.translate_amount(distance, dt);

				self.entity.translate_to(parent_transform.position, amount);
			}
		}

		match &mut self.deformation
		{
			ChildDeformation::Rigid => (),
			ChildDeformation::Stretch(strength) =>
			{
				let scale = self.entity.scale();

				let rotation = self.entity.rotation();

				let radians = rotation.cos().abs();

				let (velocity_x, velocity_y) =
					(self.entity.velocity.x.abs(), self.entity.velocity.y.abs());

				let (speed_x, speed_y) =
					(
						velocity_x * radians + velocity_y * (1.0 - radians),
						velocity_y * radians + velocity_x * (1.0 - radians)
					);

				let (ratio_x, ratio_y) = (*strength * speed_x, *strength * speed_y);

				let (stretch_x, stretch_y) =
					(
						self.scale.x + ratio_x * self.scale.x,
						self.scale.y + ratio_y * self.scale.y
					);

				self.entity.set_scale(Vector3::new(stretch_x, stretch_y, scale.z));
			}
		}
	}
}

impl OnTransformCallback for ChildEntity
{
	fn callback(&mut self) {}
}

impl TransformContainer for ChildEntity
{
	fn transform_ref(&self) -> &Transform
	{
		self.entity.transform_ref()
	}

	fn transform_mut(&mut self) -> &mut Transform
	{
		self.entity.transform_mut()
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

	fn update(&mut self, _dt: f32) {}

	fn velocity_add(&mut self, velocity: Vector3<f32>)
	{
		self.entity_mut().velocity += velocity;
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
	fn callback(&mut self)
	{
		self.children.iter_mut().for_each(|child| child.callback());
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

	fn set_rotation(&mut self, rotation: f32)
	{
		self.transform_mut().rotation = rotation;
		self.children.iter_mut().for_each(|child| child.set_rotation(rotation));

		self.callback();
	}

	fn rotate(&mut self, radians: f32)
	{
		self.transform_mut().rotation += radians;
		self.children.iter_mut().for_each(|child| child.rotate(radians));

		self.callback();
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

		let transform = self.transform_clone();
		self.children.iter_mut().for_each(|child|
		{
			child.update(transform.clone(), dt);
		});

		self.callback();
	}
}

impl DrawableEntity for Entity
{
	fn texture(&self) -> &str
	{
		&self.texture
	}
}