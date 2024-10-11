use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

use crate::{
    debug_config::*,
    common::{
        project_onto,
        project_onto_plane,
        short_rotation,
        collider::*,
        Entity
    }
};


const HINGE_EPSILON: f32 = 0.002;
const LIMIT_MAX: f32 = 0.01;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HingeAngleLimit
{
    pub base: f32,
    pub distance: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HingeJoint
{
    pub origin: Vector3<f32>,
    pub angle_limit: Option<HingeAngleLimit>
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
        let normal = -Unit::new_unchecked(diff / magnitude);

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
        let angle_local = short_rotation(this.rotation - angle_limit.base);
        if angle_local.abs() > angle_limit.distance
        {
            let point = project_onto(this, &-joint.origin);

            // perpendicular to the angle normal
            let angle_local = angle_local.clamp(-angle_limit.distance, angle_limit.distance);

            let angle = angle_local + angle_limit.base;
            let plane_normal = Unit::new_unchecked(Vector3::new(-angle.sin(), angle.cos(), 0.0));

            let projected = project_onto_plane(plane_normal, 0.0, point - this.position);

            let diff = (projected + this.position) - point;
            let penetration = diff.magnitude();

            if penetration < HINGE_EPSILON
            {
                return;
            }

            let normal = Unit::new_unchecked(diff / penetration);

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
        if DebugConfig::is_enabled(DebugTool::NoJoints)
        {
            return;
        }

        match self
        {
            Self::Hinge(joint) => hinge_contact(transform, entity, base, joint, contacts)
        }
    }
}
