use serde::{Serialize, Deserialize};

use nalgebra::{Rotation, Vector2};

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
	pub limit: f32,
    pub damping: f32,
	pub strength: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StretchDeformation
{
	pub animation: ValueAnimation,
	pub limit: f32,
	pub strength: f32
}

impl StretchDeformation
{
	pub fn stretch(&self, velocity: Vector3<f32>) -> (f32, Vector2<f32>)
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
pub enum ChildRotation
{
    Instant
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

		match &self.connection
		{
			ChildConnection::Rigid =>
            {
                self.entity.physical = parent_physical.clone();
                self.entity.physical.transform.position += origin;
            },
			ChildConnection::Spring(connection) =>
			{
                let target_position = parent_physical.position() + origin;

                let distance = target_position - self.position();

                let spring_force = distance * connection.strength;

                self.entity.add_force(spring_force);
                self.entity.damp_velocity(connection.damping, dt);
			}
		}

        match &self.rotation
        {
            ChildRotation::Instant =>
            {
                self.set_rotation(parent_physical.rotation());
                self.set_rotation_axis(*parent_physical.rotation_axis());
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

        self.entity.physics_update(dt);

        self.entity.physical.transform.position.z = parent_physical.transform.position.z;
	}
}

entity_forward!{ChildEntity, entity}
