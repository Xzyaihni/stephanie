use serde::{Serialize, Deserialize};

use yanyaengine::{Object, Transform};

use crate::common::ObjectsStore;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity(usize);

pub type EntityId = ();

pub struct Components
{
    transforms: ObjectsStore<Transform>
}
