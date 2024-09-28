use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

use crate::common::{
    project_onto,
    project_onto_plane,
    short_rotation,
    collider::*,
    Entity
};


const HINGE_EPSILON: f32 = 0.002;
const LIMIT_MAX: f32 = 0.01;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HingeJoint
{
    pub origin: Vector3<f32>,
    pub angle_limit: Option<f32>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Joint
{
    Hinge(HingeJoint)
}

fn hinge_contact(
    this: &Transform,
    entity: Entity,
    base: Vector3<f32>,
    joint: &HingeJoint,
    contacts: &mut Vec<Contact>
)
{
    let pos = project_onto(this, &joint.origin);

    let diff = pos - base;

    let magnitude = diff.magnitude();

    if magnitude > HINGE_EPSILON
    {
        let normal = -(diff / magnitude);

        contacts.push(Contact{
            a: entity,
            b: None,
            point: pos,
            penetration: magnitude - HINGE_EPSILON,
            normal
        });
    }

    if let Some(angle_limit) = joint.angle_limit
    {
        let angle = short_rotation(this.rotation);
        if angle.abs() > angle_limit
        {
            let point = project_onto(this, &-joint.origin);

            // perpendicular to the angle normal
            let angle = angle.clamp(-angle_limit, angle_limit);
            let plane_normal = Unit::new_unchecked(Vector3::new(-angle.sin(), angle.cos(), 0.0));

            let projected = project_onto_plane(plane_normal, 0.0, point - this.position);

            let diff = (projected + this.position) - point;
            let penetration = diff.magnitude();

            if penetration < HINGE_EPSILON
            {
                return;
            }

            let normal = diff / magnitude;

            contacts.push(Contact{
                a: entity,
                b: None,
                point,
                penetration: (penetration - HINGE_EPSILON).min(LIMIT_MAX),
                normal
            });
        }
    }
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
        match self
        {
            Self::Hinge(joint) => hinge_contact(transform, entity, base, joint, contacts)
        }
    }
}
