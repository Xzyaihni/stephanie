use serde::{Serialize, Deserialize};

use transform::{Transform, TransformContainer};

pub mod transform;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity
{
	transform: Transform
}

impl Entity
{
	pub fn new() -> Self
	{
		Self{transform: Transform::new()}
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

	fn callback(&mut self) {}
}