use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    rotate_point_z_3d,
    collider::*,
    Entity
};


const HINGE_EPSILON: f32 = 0.002;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Joint
{
    Hinge{origin: Vector3<f32>}
}

fn hinge_contact(
    this: &Transform,
    entity: Entity,
    base: Vector3<f32>,
    origin: &Vector3<f32>
) -> Option<Contact>
{
    let origin_local = this.scale.component_mul(origin);
    let pos = rotate_point_z_3d(origin_local, this.rotation) + this.position;

    let diff = pos - base;

    let magnitude = diff.magnitude();

    if magnitude < HINGE_EPSILON
    {
        return None;
    }

    let normal = -(diff / magnitude);

    Some(Contact{
        a: entity,
        b: None,
        point: base,
        penetration: magnitude - HINGE_EPSILON,
        normal
    })
}

impl Joint
{
    pub fn add_contacts(
        &self,
        transform: &Transform,
        entity: Entity,
        base: Vector3<f32>,
        contacts: &mut Vec<Contact>
    )
    {
        let maybe_contact = match self
        {
            Self::Hinge{origin} => hinge_contact(transform, entity, base, origin)
        };

        contacts.extend(maybe_contact);
    }
}
