use std::fmt::{self, Display};

use image::{Rgba, DynamicImage, RgbaImage};

use nalgebra::Vector2;

use yanyaengine::TextureId;

use super::PartCreator;
use crate::{
    debug_config::*,
    client::UiElementShapeMask,
    common::{anatomy::*, Side1d}
};


pub struct UiAnatomyLocation
{
    pub id: TextureId,
    pub mask: UiElementShapeMask
}

impl UiAnatomyLocation
{
    fn from_color(
        part_creator: &mut PartCreator,
        base_image: &RgbaImage,
        color: Rgba<u8>
    ) -> Self
    {
        let size = Vector2::new(base_image.width() as usize, base_image.height() as usize);
        let mut mask = UiElementShapeMask::new_empty(size);

        let mut image = base_image.clone();
        image.enumerate_pixels_mut().for_each(|(x, y, pixel)|
        {
            let new_pixel = if *pixel == color
            {
                *mask.get_mut(Vector2::new(x as usize, y as usize)).unwrap() = true;

                Rgba([u8::MAX; 4])
            } else
            {
                Rgba([0; 4])
            };

            *pixel = new_pixel;
        });

        let id = part_creator.create(image);

        Self{
            id,
            mask
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnatomyChangedPart
{
    Exact(ChangedPart),
    Brain(Side1d)
}

impl From<ChangedPart> for AnatomyChangedPart
{
    fn from(x: ChangedPart) -> Self
    {
        if let ChangedPart::Organ(OrganId::Brain(side, _)) = &x
        {
            return Self::Brain(*side);
        }

        Self::Exact(x)
    }
}

impl Display for AnatomyChangedPart
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            Self::Exact(x) => Display::fmt(x, f),
            Self::Brain(side) => write!(f, "{side} brain hemisphere")
        }
    }
}

fn color_pairs() -> Vec<(AnatomyChangedPart, Rgba<u8>)>
{
    let parts: Vec<_> = ChangedPart::iter().filter_map(|x|
    {
        if let ChangedPart::Organ(OrganId::Brain(_, _)) = x
        {
            return None;
        }

        Some(AnatomyChangedPart::Exact(x))
    }).chain([AnatomyChangedPart::Brain(Side1d::Left), AnatomyChangedPart::Brain(Side1d::Right)])
        .collect();

    let per_channel = (parts.len() as f64).cbrt().ceil() as usize;
    parts.into_iter().enumerate().map(|(index, key)|
    {
        let index_to_c = |i: usize| -> u8
        {
            let step = 255.0 / (per_channel - 1) as f64;

            (step * i as f64).round().clamp(0.0, u8::MAX as f64) as u8
        };

        let r = index_to_c(index / (per_channel * per_channel));
        let g = index_to_c((index / per_channel) % per_channel);
        let b = index_to_c(index % per_channel);

        let color = Rgba([r, g, b, u8::MAX]);

        (key, color)
    }).collect()
}

pub struct UiAnatomyLocations
{
    pub locations: Vec<(AnatomyChangedPart, UiAnatomyLocation)>
}

impl UiAnatomyLocations
{
    pub fn new(
        mut part_creator: PartCreator,
        base_image: DynamicImage
    ) -> Self
    {
        let base_image = base_image.into_rgba8();

        let color_pairs = color_pairs();

        if DebugConfig::is_enabled(DebugTool::PrintAnatomyColors)
        {
            color_pairs.iter().for_each(|(name, color)|
            {
                let [r, g, b, _] = color.0;
                let h = b as u32 + ((g as u32) << 8) + ((r as u32) << 16);

                eprintln!("{name:?} has color {color:?} ({h:06x})");
            });
        }

        let locations: Vec<_> = color_pairs.into_iter().map(|(id, color)|
        {
            let location = UiAnatomyLocation::from_color(
                &mut part_creator,
                &base_image,
                color
            );

            (id, location)
        }).collect();

        Self{locations}
    }
}
