use serde::{Serialize, Deserialize};


#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorPalette(pub u32);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Palette(pub Vec<[f32; 3]>);

impl Palette
{
    pub fn closest_get(&self, index: usize) -> [f32; 3]
    {
        self.0[(self.0.len() - 1).min(index)]
    }
}
