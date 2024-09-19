use serde::{Serialize, Deserialize};

use crate::common::collider::*;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Joint
{
    Hinge
}

fn hinge_contact() -> Option<Contact>
{
    todo!()
}

impl Joint
{
    pub fn add_contacts(&self, contacts: &mut Vec<Contact>)
    {
        let maybe_contact = match self
        {
            Self::Hinge => hinge_contact()
        };

        contacts.extend(maybe_contact);
    }
}
