use std::collections::BTreeMap;

use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

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

pub mod child_entity;


#[derive(Clone)]
pub struct EntityProperties
{
	pub texture: Option<String>,
    pub physical: PhysicalProperties
}

impl EntityProperties
{
    pub fn physical(&self) -> &PhysicalProperties
    {
        &self.physical
    }
}

// derives vomit
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildId(usize);

pub type ChildrenContainer = BTreeMap<i32, (ChildEntity, ChildId)>;

pub trait ChildContainer: TransformContainer
{
	fn children_ref(&self) -> &ChildrenContainer;
	fn children_mut(&mut self) -> &mut ChildrenContainer;

    fn add_child_inner(&mut self, child: ChildEntity) -> ChildId
    {
		let this_children = self.children_mut();

        let id = ChildId(this_children.len());

        this_children.insert(child.z_level(), (child, id));

        id
    }

	fn add_child(&mut self, position: Vector3<f32>, mut child: ChildEntity) -> ChildId
	{
        {
            let mut parented = child.with_parent(self);

            parented.set_origin(position);
            parented.sync_position();
        }

        let id = self.add_child_inner(child);

		self.transform_callback(self.transform_clone());

        id
	}

	fn add_children(&mut self, children: &[ChildEntity])
	{
        children.iter().for_each(|child|
        {
            self.add_child_inner(child.clone());
        });

		self.transform_callback(self.transform_clone());
	}

    fn get_child(&self, id: ChildId) -> &ChildEntity
    {
        self.children_ref()
            .iter()
            .find_map(|(_, (child, this_id))| (*this_id == id).then_some(child))
            .expect("all ids must be valid")
    }

    fn get_child_mut(&mut self, id: ChildId) -> &mut ChildEntity
    {
        self.children_mut()
            .iter_mut()
            .find_map(|(_, (child, this_id))| (*this_id == id).then_some(child))
            .expect("all ids must be valid")
    }

    fn set_child_texture(&mut self, id: ChildId, texture: impl Into<String>)
    {
        let child = self.get_child_mut(id).entity_mut();

        child.texture = Some(texture.into());
    }
}

pub trait EntityContainer
{
    fn entity_ref(&self) -> &Entity;
    fn entity_mut(&mut self) -> &mut Entity;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity
{
	texture: Option<String>,
    physical: Physical,
	children: ChildrenContainer
}

impl Entity
{
	pub fn new(properties: EntityProperties) -> Self
	{
        let physical = Physical::from(properties.physical);

		let children = BTreeMap::new();

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
	fn children_ref(&self) -> &ChildrenContainer
	{
		&self.children
	}

	fn children_mut(&mut self) -> &mut ChildrenContainer
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
        if !self.physical.floating
        {
            self.physical.grounded = true;
        }

        self.physical_mut().physics_update(dt);

		self.children.iter_mut().for_each(|(_, (child, _))|
		{
			child.update(&self.physical, dt);
		});
    }
}

impl DrawableEntity for Entity
{
	fn texture(&self) -> Option<&str>
	{
		self.texture.as_deref()
	}

    fn needs_redraw(&mut self) -> bool
    {
        false
    }
}

#[macro_export]
macro_rules! entity_forward_physics
{
    ($name:ident, $child_name:ident) =>
    {
        use $crate::common::{
            Physical,
            physics::PhysicsEntity
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
    }
}

#[macro_export]
macro_rules! entity_forward_transform
{
    ($name:ident, $child_name:ident) =>
    {
        use nalgebra::{
            Unit,
            Vector3
        };

        use yanyaengine::{Transform, TransformContainer, OnTransformCallback};

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
    }
}

#[macro_export]
macro_rules! entity_forward_parent
{
    ($name:ident, $child_name:ident) =>
    {
        use $crate::common::{
            ChildContainer,
            entity::{
                Entity,
                EntityContainer,
                ChildrenContainer
            }
        };

        impl ChildContainer for $name
        {
            fn children_ref(&self) -> &ChildrenContainer
            {
                self.$child_name.children_ref()
            }

            fn children_mut(&mut self) -> &mut ChildrenContainer
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

#[macro_export]
macro_rules! basic_entity_forward
{
    ($name:ident, $child_name:ident) =>
    {
        use $crate::{
            entity_forward_transform,
            entity_forward_physics,
            entity_forward_parent
        };

        entity_forward_parent!{$name, $child_name}
        entity_forward_transform!{$name, $child_name}
        entity_forward_physics!{$name, $child_name}
    }
}

#[macro_export]
macro_rules! entity_forward_drawable
{
    ($name:ident, $child_name:ident) =>
    {
        use $crate::client::DrawableEntity;

        impl DrawableEntity for $name
        {
            fn texture(&self) -> Option<&str>
            {
                self.$child_name.texture()
            }

            fn needs_redraw(&mut self) -> bool
            {
                self.$child_name.needs_redraw()
            }
        }
    }
}

#[macro_export]
macro_rules! entity_forward
{
    ($name:ident, $child_name:ident) =>
    {
        use $crate::{
            basic_entity_forward,
            entity_forward_drawable
        };

        basic_entity_forward!{$name, $child_name}
        entity_forward_drawable!{$name, $child_name}
    }
}
