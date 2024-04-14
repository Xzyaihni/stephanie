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
pub struct EaseOutRotation
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
    EaseOut(EaseOutRotation),
    Constant{speed: f32}
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
        let new_position = self.parent.position() + self.entity.origin();

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
    origin_rotation: f32,
	entity: Entity,
	z_level: i32
}

impl ChildEntity
{
	pub fn new(
		connection: ChildConnection,
        rotation: ChildRotation,
		deformation: ChildDeformation,
		entity: Entity,
		z_level: i32
	) -> Self
	{
		Self{
            connection,
            rotation,
            deformation,
            origin: Vector3::zeros(),
            origin_rotation: entity.rotation(),
            entity,
            z_level
        }
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

	fn origin(&self) -> Vector3<f32>
	{
        let rotation = Rotation::from_axis_angle(
            &self.rotation_axis(),
            self.rotation() - self.origin_rotation
        );

		rotation * self.origin
	}

	pub fn update(&mut self, parent_physical: &Physical, dt: f32)
	{
		match &self.deformation
		{
			ChildDeformation::Rigid => (),
			ChildDeformation::Stretch(deformation) =>
			{
				let stretch = deformation.stretch(self.entity.physical.velocity);

				self.entity.set_stretch(stretch);
			}
		}

        let target_rotation = parent_physical.rotation() + self.origin_rotation;

        self.set_rotation_axis(*parent_physical.rotation_axis());

        match &self.rotation
        {
            ChildRotation::Instant =>
            {
                self.set_rotation(target_rotation);
            },
            ChildRotation::EaseOut(..) | ChildRotation::Constant{..} =>
            {
                let rotation_difference = target_rotation - self.rotation();
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

                let target_rotation = rotation_difference + self.rotation();

                match &self.rotation
                {
                    ChildRotation::EaseOut(props) =>
                    {
                        let amount = 1.0 - props.strength.powf(dt);

                        let rotation = lerp(self.rotation(), target_rotation, amount);

                        self.set_rotation(rotation);
                    },
                    ChildRotation::Constant{speed} =>
                    {
                        let distance = target_rotation - self.rotation();

                        let max_move = speed * dt;
                        let move_amount = distance.clamp(-max_move, max_move);

                        let rotation = self.rotation() + move_amount;

                        self.set_rotation(rotation);
                    },
                    _ => unreachable!()
                }
            }
        }

        let target_position = |this: &Self|
        {
            parent_physical.position() + this.origin()
        };

		match &self.connection
		{
			ChildConnection::Rigid =>
            {
                self.transform_mut().position = target_position(self);
            },
			ChildConnection::Spring(connection) =>
			{
                let distance = target_position(self) - self.position();

                let spring_force = distance * connection.strength;

                self.entity.add_force(spring_force);
                self.entity.damp_velocity(connection.damping, dt);

                self.entity.physics_update(dt);

                if distance.magnitude() > connection.limit
                {
                    self.clamp_distance(target_position(self), connection.limit);
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
