use serde::{Serialize, Deserialize};

use crate::common::items_info::ItemId;


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item
{
    pub id: ItemId
}
