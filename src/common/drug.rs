use serde::Deserialize;


#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum Drug
{
    Heal{amount: f32},
    BoneHeal{amount: u32}
}
