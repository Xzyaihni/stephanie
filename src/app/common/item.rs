use serde::{Serialize, Deserialize};

use crate::common::items_info::ItemId;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item
{
    id: ItemId
}
