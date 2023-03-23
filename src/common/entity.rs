use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use transform::{Transform, OnTransformCallback, TransformContainer};

use crate::common::physics::PhysicsEntity;

pub mod transform;


#[derive(Debug, Clone)]
pub struct EntityProperties
{
	pub damp_factor: f32
}

impl Default for EntityProperties
{
	fn default() -> Self
	{
		Self{damp_factor: 0.5}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity
{
	damp_factor: f32,
	transform: Transform,
	pub velocity: Vector3<f32>
}

impl Entity
{
	pub fn new(properties: EntityProperties) -> Self
	{
		let damp_factor = properties.damp_factor;

		let velocity = Vector3::zeros();

		Self{damp_factor, transform: Transform::new(), velocity}
	}
}

impl OnTransformCallback for Entity
{
	fn callback(&mut self) {}
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
		let damp = self.damp_factor.powf(dt);

		self.translate(self.velocity * (damp - 1.0) / self.damp_factor.ln());
		self.velocity *= damp;
	}
}