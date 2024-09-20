use serde::{Serialize, Deserialize};

use nalgebra::Vector3;

use yanyaengine::Transform;

use crate::common::{
    PENETRATION_EPSILON,
    rotate_point_z_3d,
    collider::*,
    Entity
};


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

    if magnitude < PENETRATION_EPSILON.general
    {
        return None;
    }

    let normal = -(diff / magnitude);

    Some(Contact{
        a: entity,
        b: None,
        point: base,
        penetration: magnitude,
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

        if let Self::Hinge{origin} = self
        {
            let origin_local = transform.scale.component_mul(origin);
            let pos = rotate_point_z_3d(origin_local, transform.rotation) + transform.position;

            contacts.push(Contact{
                a: entity,
                b: None,
                point: pos,
                penetration: -1.0,
                normal: Vector3::y()
            });
        }

        contacts.extend(maybe_contact);
    }
}
