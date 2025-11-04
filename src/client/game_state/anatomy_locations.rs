use image::{Rgba, DynamicImage, RgbaImage};

use nalgebra::Vector2;

use yanyaengine::{TextureId, object::texture::{outline_image, Imageable, ImageOutline, Color}};

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

fn color_pairs() -> Vec<(ChangedPart, Rgba<u8>)>
{
    let parts: Vec<_> = ChangedPart::iter().filter(|x|
    {
        if let ChangedPart::Organ(_) = x
        {
            false
        } else
        {
            true
        }
    }).chain([
        ChangedPart::Organ(OrganId::Brain(Some(Side1d::Left), None)),
        ChangedPart::Organ(OrganId::Brain(Some(Side1d::Right), None))
    ]).chain(OrganId::iter().filter(|x|
    {
        if let OrganId::Brain(_, _) = x
        {
            false
        } else
        {
            true
        }
    }).map(ChangedPart::Organ)).collect();

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

struct OutlineGenerator<'a>(&'a RgbaImage);

impl Imageable for OutlineGenerator<'_>
{
    fn width(&self) -> usize { self.0.width() as usize }
    fn height(&self) -> usize { self.0.height() as usize }

    fn get_pixel(&self, x: usize, y: usize) -> Color
    {
        Color{
            r: 255,
            g: 255,
            b: 255,
            a: self.0.get_pixel(x as u32, y as u32).0[3]
        }
    }
}

pub struct UiAnatomyLocations
{
    pub full: TextureId,
    pub outline: TextureId,
    pub locations: Vec<(ChangedPart, UiAnatomyLocation)>
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

        let full = part_creator.create(base_image.clone());

        let outline = {
            let image = OutlineGenerator(&base_image);
            part_creator.create(outline_image::<true>(
                &image,
                ImageOutline{color: [255; 3], size: 2}
            ).expect("outline must not be 0"))
        };

        let locations: Vec<_> = color_pairs.into_iter().map(|(id, color)|
        {
            let location = UiAnatomyLocation::from_color(
                &mut part_creator,
                &base_image,
                color
            );

            (id, location)
        }).collect();

        Self{full, outline, locations}
    }
}
