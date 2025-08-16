use crate::common::colors::Lch;


pub enum SkyLight
{
    Day,
    Sunset(f64),
    Night,
    Sunrise(f64)
}

impl Default for SkyLight
{
    fn default() -> Self
    {
        Self::Day
    }
}

impl SkyLight
{
    pub fn brightness(&self) -> f64
    {
        match self
        {
            Self::Day => 1.0,
            Self::Sunset(progress) => 1.0 - *progress,
            Self::Night => 0.0,
            Self::Sunrise(progress) => *progress
        }
    }

    pub fn light_color(&self) -> [f32; 3]
    {
        match self
        {
            Self::Day => [1.0, 1.0, 1.0],
            Self::Night => [0.0, 0.0, 0.0],
            Self::Sunset(_)
            | Self::Sunrise(_) =>
            {
                let x = (1.0 - self.brightness()) as f32;

                let orange_start = 0.3;
                let chroma_ends = 0.4;

                let l = (if x < orange_start { 1.0 } else { 1.0 - (x - orange_start) / (1.0 - orange_start) }) * 100.0;
                let c = (if x < chroma_ends { (x / orange_start).min(1.0) } else { 1.0 - (x - chroma_ends) / (1.0 - chroma_ends) }) * 60.0;

                Lch{l, c, h: 0.8}.into()
            }
        }
    }
}
