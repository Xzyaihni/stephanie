use std::f32;

use serde::{Serialize, Deserialize};

use nalgebra::{Rotation, Vector2};

use yanyaengine::{
    Transform,
    TransformContainer,
	OnTransformCallback
};

use crate::{entity_forward, common::lerp};


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
	pub limit: f32,
    pub damping: f32,
	pub strength: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LerpRotation
{
    pub strength: f32
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
pub enum ChildConnection
{
	Rigid,
	Spring(SpringConnection)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChildRotation
{
    Instant,
    Lerp(LerpRotation)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChildDeformation
{
	Rigid,
	Stretch(StretchDeformation)
}

pub struct ChildEntityRef<'a, P: ?Sized>
{
    parent: &'a P,
    entity: &'a mut ChildEntity
}

impl<'a, P: ?Sized> ChildEntityRef<'a, P>
where
    P: TransformContainer
{
	pub fn set_origin(&mut self, origin: Vector3<f32>)
	{
		self.entity.origin = origin.component_mul(self.parent.scale());
	}

    pub fn sync_position(&mut self)
    {
        let new_position = self.parent.position() + self.entity.origin(self.parent.transform_ref());

        self.entity.physical_mut().transform.position = new_position;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildEntity
{
	connection: ChildConnection,
    rotation: ChildRotation,
	deformation: ChildDeformation,
	origin: Vector3<f32>,
	entity: Entity,
	z_level: i32
}

impl ChildEntity
{
	pub fn new(
		connection: ChildConnection,
        rotation: ChildRotation,
		deformation: ChildDeformation,
		mut entity: Entity,
        origin: Vector3<f32>,
		z_level: i32
	) -> Self
	{
        entity.physical.transform.position += origin;

		Self{connection, rotation, deformation, origin, entity, z_level}
	}

    pub fn with_parent<'a, P: ?Sized>(&'a mut self, parent: &'a P) -> ChildEntityRef<'a, P>
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

	fn origin(&self, parent_transform: &Transform) -> Vector3<f32>
	{
        let rotation = Rotation::from_axis_angle(
            &parent_transform.rotation_axis,
            parent_transform.rotation
        );

		rotation * self.origin
	}

	pub fn update(&mut self, parent_physical: &Physical, dt: f32)
	{
        let origin = self.origin(&parent_physical.transform);

        self.set_rotation_axis(*parent_physical.rotation_axis());
        match &self.rotation
        {
            ChildRotation::Instant =>
            {
                self.set_rotation(parent_physical.rotation());
            },
            ChildRotation::Lerp(props) =>
            {
                let rotation_difference = parent_physical.rotation() - self.rotation();
                let rotation_difference = if rotation_difference > f32::consts::PI
                {
                    rotation_difference - 2.0 * f32::consts::PI
                } else if rotation_difference < -f32::consts::PI
                {
                    rotation_difference + 2.0 * f32::consts::PI
                } else
                {
                    rotation_difference
                };

                let amount = 1.0 - props.strength.powf(dt);

                let rotation = self.rotation() + lerp(0.0, rotation_difference, amount);

                self.set_rotation(rotation);
            }
        }

		match &self.deformation
		{
			ChildDeformation::Rigid => (),
			ChildDeformation::Stretch(deformation) =>
			{
				let stretch = deformation.stretch(self.entity.physical.velocity);

				self.entity.set_stretch(stretch);
			}
		}

		match &self.connection
		{
			ChildConnection::Rigid =>
            {
                self.transform_mut().position = parent_physical.position() + origin;
            },
			ChildConnection::Spring(connection) =>
			{
                let target_position = parent_physical.position() + origin;

                let distance = target_position - self.position();

                let spring_force = distance * connection.strength;

                self.entity.add_force(spring_force);
                self.entity.damp_velocity(connection.damping, dt);

                self.entity.physics_update(dt);

                if distance.magnitude() > connection.limit
                {
                    let target_position = parent_physical.position() + origin;

                    self.clamp_distance(target_position, connection.limit);
                }

                self.entity.physical.transform.position.z = parent_physical.transform.position.z;
			}
		}
	}

    fn clamp_distance(&mut self, target_position: Vector3<f32>, limit: f32)
    {
        let distance = target_position - self.position();

        // checking again cuz this is after the physics update
        if distance.magnitude() < limit
        {
            return;
        }

        let limited_position = distance.normalize() * limit;

        self.transform_mut().position = target_position - limited_position;
    }
}

entity_forward!{ChildEntity, entity}
