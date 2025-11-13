use serde::Deserialize;


#[derive(Debug, Clone, Deserialize)]
pub enum Drug
{
    Heal{amount: f32},
    BoneHeal{amount: u32}
}
