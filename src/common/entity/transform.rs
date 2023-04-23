use serde::{Serialize, Deserialize};

use nalgebra::{
	Unit,
	Vector2,
	Vector3
};

use super::ChildEntity;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform
{
	pub rotation_axis: Unit<Vector3<f32>>,
	pub rotation: f32,
	pub position: Vector3<f32>,
	pub scale: Vector3<f32>,
	pub stretch: (f32, Vector2<f32>)
}

impl Transform
{
	pub fn new() -> Self
	{
		let rotation_axis = Unit::new_normalize(Vector3::z());
		let rotation = 0.0;

		let position = Vector3::zeros();
		let scale = Vector3::new(1.0, 1.0, 1.0);

		let stretch = (0.0, Vector2::new(1.0, 1.0));

		Self{rotation_axis, rotation, position, scale, stretch}
	}

	pub fn half(&self) -> Vector3<f32>
	{
		self.scale / 2.0
	}

	pub fn distance(&self, value: Vector3<f32>) -> f32
	{
		Self::distance_associated(self.position, value)
	}

	pub fn direction(&self, value: Vector3<f32>) -> Vector3<f32>
	{
		value - self.position
	}

	pub fn interpolate(value0: f32, value1: f32, amount: f32) -> f32
	{
		value0 * (1.0 - amount) + value1 * amount
	}

	pub fn interpolate_vector(
		value0: Vector3<f32>,
		value1: Vector3<f32>,
		amount: f32
	) -> Vector3<f32>
	{
		Vector3::new(
			Self::interpolate(value0.x, value1.x, amount),
			Self::interpolate(value0.y, value1.y, amount),
			Self::interpolate(value0.z, value1.z, amount)
		)
	}

	pub fn distance_associated(value0: Vector3<f32>, value1: Vector3<f32>) -> f32
	{
		(value1 - value0).magnitude()
	}
}

pub trait ChildContainer: TransformContainer
{
	fn children_ref(&self) -> &[ChildEntity];
	fn children_mut(&mut self) -> &mut Vec<ChildEntity>;

	fn add_child(&mut self, child: ChildEntity)
	{
		self.add_children(&mut [child]);
	}

	fn add_children(&mut self, children: &mut [ChildEntity])
	{
		children.iter_mut().for_each(|child| child.transform_callback(self.transform_clone()));

		let this_children = self.children_mut();

		this_children.extend(children.iter().cloned());
		this_children.sort_by(|child, other| child.z_level().cmp(&other.z_level()));
	}
}

pub trait OnTransformCallback
{
	fn callback(&mut self) {}

	fn transform_callback(&mut self, _transform: Transform)
	{
		self.callback();
	}

	fn position_callback(&mut self, _position: Vector3<f32>)
	{
		self.callback();
	}

	fn scale_callback(&mut self, _scale: Vector3<f32>)
	{
		self.callback();
	}

	fn rotation_callback(&mut self, _rotation: f32)
	{
		self.callback();
	}

	fn rotation_axis_callback(&mut self, _axis: Unit<Vector3<f32>>)
	{
		self.callback();
	}

	fn stretch_callback(&mut self, _stretch: (f32, Vector2<f32>))
	{
		self.callback();
	}
}

#[allow(dead_code)]
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
		self.set_transform_only(transform.clone());
		self.transform_callback(transform);
	}

	fn set_transform_only(&mut self, transform: Transform)
	{
		*self.transform_mut() = transform;
	}

	fn position(&self) -> &Vector3<f32>
	{
		&self.transform_ref().position
	}

	fn interpolate_position(&self, value: Vector3<f32>, amount: f32) -> Vector3<f32>
	{
		Transform::interpolate_vector(self.transform_ref().position, value, amount)
	}

	fn translate_to(&mut self, value: Vector3<f32>, amount: f32)
	{
		let new_position = self.interpolate_position(value, amount);

		self.set_position(new_position);
	}

	fn distance(&self, value: Vector3<f32>) -> f32
	{
		self.transform_ref().distance(value)
	}

	fn direction(&self, value: Vector3<f32>) -> Vector3<f32>
	{
		self.transform_ref().direction(value)
	}

	fn translate(&mut self, position: Vector3<f32>)
	{
		self.set_position(self.position() + position);
	}

	fn set_position(&mut self, position: Vector3<f32>)
	{
		self.transform_mut().position = position;
		self.position_callback(position);
	}

	fn scale(&self) -> &Vector3<f32>
	{
		&self.transform_ref().scale
	}

	fn set_scale(&mut self, scale: Vector3<f32>)
	{
		self.transform_mut().scale = scale;
		self.scale_callback(scale);
	}

	fn grow(&mut self, scale: Vector3<f32>)
	{
		self.set_scale(self.scale() + scale);
	}

	fn rotation_axis(&self) -> &Unit<Vector3<f32>>
	{
		&self.transform_ref().rotation_axis
	}

	fn set_rotation_axis(&mut self, axis: Unit<Vector3<f32>>)
	{
		self.transform_mut().rotation_axis = axis;
		self.rotation_axis_callback(axis);
	}

	fn rotation(&self) -> f32
	{
		self.transform_ref().rotation
	}

	fn set_rotation(&mut self, rotation: f32)
	{
		self.transform_mut().rotation = rotation;
		self.rotation_callback(rotation);
	}

	fn half(&self) -> Vector3<f32>
	{
		self.transform_ref().half()
	}

	fn set_stretch(&mut self, stretch: (f32, Vector2<f32>))
	{
		self.transform_mut().stretch = stretch;
		self.stretch_callback(stretch);
	}
}