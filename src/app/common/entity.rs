use serde::{Serialize, Deserialize};

use yanyaengine::{
    Transform,
    TransformContainer,
	OnTransformCallback
};

use crate::{
	client::DrawableEntity,
	common::physics::PhysicsEntity
};

pub use crate::common::physics::{
    PhysicalProperties,
    Physical
};

pub use child_entity::*;

mod child_entity;


pub struct EntityProperties
{
	pub texture: String,
    pub physical: PhysicalProperties
}

impl EntityProperties
{
    pub fn physical(&self) -> &PhysicalProperties
    {
        &self.physical
    }
}

pub trait ChildContainer: TransformContainer
{
	fn children_ref(&self) -> &[ChildEntity];
	fn children_mut(&mut self) -> &mut Vec<ChildEntity>;

    fn add_child_inner(&mut self, child: ChildEntity)
    {
		let this_children = self.children_mut();

        let index = this_children.binary_search_by(|other|
        {
            other.z_level().cmp(&child.z_level())
        }).unwrap_or_else(|partition| partition);

        this_children.insert(index, child);
    }

	fn add_child(&mut self, child: ChildEntity)
	{
        self.add_child_inner(child);

		self.transform_callback(self.transform_clone());
	}

	fn add_children(&mut self, children: &[ChildEntity])
	{
        children.into_iter().for_each(|child|
        {
            self.add_child_inner(child.clone());
        });

		self.transform_callback(self.transform_clone());
	}
}

pub trait EntityContainer: PhysicsEntity + DrawableEntity + ChildContainer
{
    fn entity_ref(&self) -> &Entity;
    fn entity_mut(&mut self) -> &mut Entity;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity
{
	texture: String,
    physical: Physical,
	children: Vec<ChildEntity>
}

impl Entity
{
	pub fn new(properties: EntityProperties) -> Self
	{
        let physical = Physical::from(properties.physical);

		let children = Vec::new();

		Self{
            texture: properties.texture,
            physical,
            children
        }
	}
}

impl EntityContainer for Entity
{
    fn entity_ref(&self) -> &Entity
    {
        self
    }

    fn entity_mut(&mut self) -> &mut Entity
    {
        self
    }
}

impl OnTransformCallback for Entity {}

impl TransformContainer for Entity
{
	fn transform_ref(&self) -> &Transform
	{
		self.physical.transform_ref()
	}

	fn transform_mut(&mut self) -> &mut Transform
	{
		self.physical.transform_mut()
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
	fn physical_ref(&self) -> &Physical
    {
        &self.physical
    }

	fn physical_mut(&mut self) -> &mut Physical
    {
        &mut self.physical
    }

	fn physics_update(&mut self, dt: f32)
    {
        // remove this after i add collisions
        if !self.children.is_empty()
        {
            self.physical.grounded = true;
        }

        self.physical_mut().physics_update(dt);

		self.children.iter_mut().for_each(|child|
		{
			child.update(&self.physical, dt);
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

#[macro_export]
macro_rules! entity_forward
{
    ($name:ident, $child_name:ident) =>
    {
        use nalgebra::{
            Unit,
            Vector3
        };

        use crate::{
            client::DrawableEntity,
            common::{
                Physical,
                ChildContainer,
                entity::{
                    Entity,
                    EntityContainer
                },
                physics::PhysicsEntity
            }
        };

        impl PhysicsEntity for $name
        {
            fn physical_ref(&self) -> &Physical
            {
                self.$child_name.physical_ref()
            }

            fn physical_mut(&mut self) -> &mut Physical
            {
                self.$child_name.physical_mut()
            }

            fn physics_update(&mut self, dt: f32)
            {
                self.$child_name.physics_update(dt);
            }
        }

        impl DrawableEntity for $name
        {
            fn texture(&self) -> &str
            {
                self.$child_name.texture()
            }
        }

        impl OnTransformCallback for $name
        {
            fn transform_callback(&mut self, transform: Transform)
            {
                self.$child_name.transform_callback(transform);
            }

            fn position_callback(&mut self, position: Vector3<f32>)
            {
                self.$child_name.position_callback(position);
            }

            fn scale_callback(&mut self, scale: Vector3<f32>)
            {
                self.$child_name.scale_callback(scale);
            }

            fn rotation_callback(&mut self, rotation: f32)
            {
                self.$child_name.rotation_callback(rotation);
            }

            fn rotation_axis_callback(&mut self, axis: Unit<Vector3<f32>>)
            {
                self.$child_name.rotation_axis_callback(axis);
            }
        }

        impl TransformContainer for $name
        {
            fn transform_ref(&self) -> &Transform
            {
                self.$child_name.transform_ref()
            }

            fn transform_mut(&mut self) -> &mut Transform
            {
                self.$child_name.transform_mut()
            }
        }

        impl ChildContainer for $name
        {
            fn children_ref(&self) -> &[crate::common::ChildEntity]
            {
                self.$child_name.children_ref()
            }

            fn children_mut(&mut self) -> &mut Vec<crate::common::ChildEntity>
            {
                self.$child_name.children_mut()
            }
        }

        impl EntityContainer for $name
        {
            fn entity_ref(&self) -> &Entity
            {
                self.$child_name.entity_ref()
            }

            fn entity_mut(&mut self) -> &mut Entity
            {
                self.$child_name.entity_mut()
            }
        }
    }
}
