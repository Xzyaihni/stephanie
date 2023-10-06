use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3, Rotation};

use yanyaengine::{
    Transform,
    TransformContainer,
	OnTransformCallback,
    object::Model
};

use crate::{
	client::DrawableEntity,
	common::physics::PhysicsEntity
};


#[derive(Debug, Clone, Serialize, Deserialize)]
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
		let transform = Transform::default();

		let texture = String::new();

		Self{transform, texture, damp_factor: 0.5}
	}
}

fn limit_distance(limit: f32, distance: f32) -> f32
{
	(1.0 - (limit / distance)).max(0.0)
}

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
			ValueAnimation::Linear => value,
			ValueAnimation::EaseIn(strength) => value.powf(*strength),
			ValueAnimation::EaseOut(strength) => 1.0 - (1.0 - value).powf(*strength)
		}
	}
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
	#[allow(dead_code)]
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

#[allow(dead_code)]
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

	pub fn stretched(&mut self, velocity: Vector3<f32>) -> (f32, Vector2<f32>)
	{
		let amount = self.animation.apply(velocity.magnitude() * self.strength);
		let stretch = (1.0 + amount).max(self.limit);

		let angle = velocity.y.atan2(-velocity.x);

		(angle, Vector2::new(stretch, 1.0 / stretch))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetStretchDeformation
{
	animation: ValueAnimation,
	limit: f32,
	strength: f32,
	stretchiness: f32
}

impl OffsetStretchDeformation
{
    #[allow(dead_code)]
	pub fn new(animation: ValueAnimation, limit: f32, strength: f32, stretchiness: f32) -> Self
	{
		Self{animation, limit, strength, stretchiness}
	}

	pub fn stretched(&self, model: &mut Model, velocity: &mut Vector3<f32>, dt: f32)
	{
		let _ = ChildEntity::damp_velocity(
			velocity,
			self.stretchiness,
			dt
		);

		velocity.x = velocity.x.clamp(0.0, 1.0 / self.strength);
		velocity.y = velocity.y.clamp(-1.0 / self.strength, 1.0 / self.strength);

		let x_amount = self.animation.apply(velocity.x * self.strength);
		let y_amount = self.animation.apply(velocity.y.abs() * self.strength);

		let x_offset = -x_amount * self.limit;
		let y_offset = if velocity.y > 0.0
		{
			-y_amount * self.limit
		} else
		{
			y_amount * self.limit
		};

		model.vertices[0][0] = model.vertices[2][0] + x_offset;
		model.vertices[0][1] = model.vertices[2][1] + y_offset;

		model.vertices[1][0] = model.vertices[4][0] + x_offset;
		model.vertices[1][1] = model.vertices[4][1] + y_offset;

		model.vertices[3][0] = model.vertices[4][0] + x_offset;
		model.vertices[3][1] = model.vertices[4][1] + y_offset;
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

pub trait ChildContainer: TransformContainer
{
	fn children_ref(&self) -> &[ChildEntity];
	fn children_mut(&mut self) -> &mut Vec<ChildEntity>;

	fn add_child(&mut self, child: ChildEntity)
	{
		self.add_children(&[child]);
	}

	fn add_children(&mut self, children: &[ChildEntity])
	{
		let this_children = self.children_mut();

        for child in children
        {
            let index = this_children.binary_search_by(|other|
            {
                child.z_level().cmp(&other.z_level())
            }).unwrap_or_else(|partition| partition);

            this_children.insert(index, child.clone());
        }

		self.transform_callback(self.transform_clone());
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildEntity
{
	connection: ChildConnection,
	deformation: ChildDeformation,
	origin: Vector3<f32>,
	transform: Transform,
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
		let transform = entity.transform_clone();

		Self{connection, deformation, origin, transform, entity, z_level}
	}

	pub fn z_level(&self) -> i32
	{
		self.z_level
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

	fn velocity_local(&self, parent_transform: &Transform) -> Vector3<f32>
	{
		let rotation = Rotation::from_axis_angle(
			&-parent_transform.rotation_axis,
			parent_transform.rotation
		);

		rotation * self.entity.velocity
	}

	fn update(&mut self, parent_transform: &Transform, dt: f32)
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

		let velocity = self.velocity_local(parent_transform);
		match &mut self.deformation
		{
			ChildDeformation::Rigid => (),
			ChildDeformation::Stretch(deformation) =>
			{
				let stretch = deformation.stretched(velocity);

				self.entity.set_stretch(stretch);
			}
		}
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

	fn physics_update(&mut self, _dt: f32) {}
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

	fn physics_update(&mut self, dt: f32)
	{
		let translation = Self::damp_velocity(&mut self.velocity, self.damp_factor, dt);
		self.translate(translation);

		self.children.iter_mut().for_each(|child|
		{
			child.update(&self.transform, dt);
		});

		self.transform_callback(self.transform_clone());
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
