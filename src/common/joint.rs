use serde::{Serialize, Deserialize};

use nalgebra::{Unit, Vector3};

use yanyaengine::Transform;

use crate::{
    debug_config::*,
    common::{
        project_onto,
        collider::*,
        Entity
    }
};


const HINGE_EPSILON: f32 = 0.002;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HingeAngleLimit
{
    pub base: f32,
    pub distance: f32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HingeJoint
{
    pub origin: Vector3<f32>
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
