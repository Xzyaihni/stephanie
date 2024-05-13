use serde::{Serialize, Deserialize};

use nalgebra::{Vector2, Vector3};

use yanyaengine::Transform;

use crate::common::Physical;


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
pub struct StretchDeformation
{
	pub animation: ValueAnimation,
	pub limit: f32,
    pub onset: f32,
	pub strength: f32
}

impl StretchDeformation
{
	pub fn stretch(&self, velocity: Vector3<f32>) -> (f32, Vector2<f32>)
	{
		let amount = self.animation.apply(velocity.magnitude() * self.onset);
		let stretch = (1.0 + amount * self.strength).max(self.limit);

		let angle = velocity.y.atan2(-velocity.x);

		(angle, Vector2::new(stretch, 1.0 / stretch))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Deformation
{
	Rigid,
	Stretch(StretchDeformation)
}

pub struct LazyTransformInfo
{
    pub deformation: Deformation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazyTransform
{
    pub target: Transform,
    deformation: Deformation
}

impl From<LazyTransformInfo> for LazyTransform
{
    fn from(info: LazyTransformInfo) -> Self
    {
        Self{
            target: Default::default(),
            deformation: info.deformation
        }
    }
}

impl LazyTransform
{
    pub fn next(
        &mut self,
        physical: &mut Physical,
        dt: f32
    ) -> Transform
    {
        let mut output = self.target.clone();

        match &self.deformation
		{
			Deformation::Rigid => (),
			Deformation::Stretch(deformation) =>
			{
				output.stretch = deformation.stretch(physical.velocity);
			}
		}

        output
    }
}
