use serde::{Serialize, Deserialize};

use nalgebra::{
	Unit,
	base::Vector3
};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform
{
	pub rotation_axis: Unit<Vector3<f32>>,
	pub rotation: f32,
	pub position: Vector3<f32>,
	pub scale: Vector3<f32>
}

impl Transform
{
	pub fn new() -> Self
	{
		let rotation_axis = Unit::new_normalize(Vector3::z());
		let rotation = 0.0;

		let position = Vector3::zeros();
		let scale = Vector3::new(1.0, 1.0, 1.0);

		Self{rotation_axis, rotation, position, scale}
	}
}

pub trait OnTransformCallback
{
	fn callback(&mut self);
}

pub trait TransformContainer: OnTransformCallback
{
	fn transform_ref(&self) -> &Transform;
	fn transform_mut(&mut self) -> &mut Transform;

	fn transform_clone(&self) -> Transform
	{
		self.transform_ref().clone()
	}

	fn set_transform(&mut self, transform: Transform)
	{
		self.set_transform_only(transform);
		self.callback();
	}

	fn set_transform_only(&mut self, transform: Transform)
	{
		*self.transform_mut() = transform;
	}

	fn position(&self) -> &Vector3<f32>
	{
		&self.transform_ref().position
	}

	fn set_position(&mut self, position: Vector3<f32>)
	{
		self.transform_mut().position = position;
		self.callback();
	}

	fn translate(&mut self, position: Vector3<f32>)
	{
		self.transform_mut().position += position;
		self.callback();
	}

	fn scale(&self) -> &Vector3<f32>
	{
		&self.transform_ref().scale
	}

	fn set_scale(&mut self, scale: Vector3<f32>)
	{
		self.transform_mut().scale = scale;
		self.callback();
	}

	fn grow(&mut self, scale: Vector3<f32>)
	{
		self.transform_mut().scale += scale;
		self.callback();
	}

	fn rotation_axis(&self) -> &Unit<Vector3<f32>>
	{
		&self.transform_ref().rotation_axis
	}

	fn set_rotation_axis(&mut self, axis: Unit<Vector3<f32>>)
	{
		self.transform_mut().rotation_axis = axis;
		self.callback();
	}

	fn rotation(&self) -> f32
	{
		self.transform_ref().rotation
	}

	fn set_rotation(&mut self, rotation: f32)
	{
		self.transform_mut().rotation = rotation;
		self.callback();
	}

	fn rotate(&mut self, radians: f32)
	{
		self.transform_mut().rotation += radians;
		self.callback();
	}

	fn middle(&self) -> Vector3<f32>
	{
		let scale = self.transform_ref().scale;
		Vector3::new(
			scale.x / 2.0,
			scale.y / 2.0,
			0.0
		)
	}
}