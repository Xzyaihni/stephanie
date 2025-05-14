use image::{Rgba, DynamicImage, RgbaImage};

use nalgebra::Vector2;

use yanyaengine::TextureId;

use super::PartCreator;
use crate::{
    client::UiElementShapeMask,
    common::{anatomy::HumanPartId, Side1d}
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

pub struct UiAnatomyLocations
{
    pub locations: Vec<(HumanPartId, UiAnatomyLocation)>
}

impl UiAnatomyLocations
{
    pub fn new(
        mut part_creator: PartCreator,
        base_image: DynamicImage
    ) -> Self
    {
        let base_image = base_image.into_rgba8();

        let color_pairs: Vec<(HumanPartId, Rgba<u8>)> = [
            (HumanPartId::Head, 0xff0000),
            (HumanPartId::Spine, 0xdda0dd),
            (HumanPartId::Torso, 0x00008b),
            (HumanPartId::Pelvis, 0x00fa9a),
            (HumanPartId::Arm(Side1d::Right), 0xff1493),
            (HumanPartId::Arm(Side1d::Left), 0xff8c00),
            (HumanPartId::Forearm(Side1d::Right), 0x8b0000),
            (HumanPartId::Forearm(Side1d::Left), 0xffff00),
            (HumanPartId::Hand(Side1d::Right), 0x008000),
            (HumanPartId::Hand(Side1d::Left), 0x7fff00),
            (HumanPartId::Thigh(Side1d::Right), 0xe9967a),
            (HumanPartId::Thigh(Side1d::Left), 0x0000ff),
            (HumanPartId::Calf(Side1d::Right), 0x00ffff),
            (HumanPartId::Calf(Side1d::Left), 0xff00ff),
            (HumanPartId::Foot(Side1d::Right), 0x00bfff),
            (HumanPartId::Foot(Side1d::Left), 0xf0e68c),
            (HumanPartId::Eye(Side1d::Right), 0x696969),
            (HumanPartId::Eye(Side1d::Left), 0xf5f5f5)
        ].into_iter().map(|(key, value): (_, u32)|
        {
            let r = (value >> (8 * 2)) & 0xff;
            let g = (value >> 8) & 0xff;
            let b = value & 0xff;

            let color = Rgba([r as u8, g as u8, b as u8, u8::MAX]);

            (key, color)
        }).collect();

        let locations: Vec<_> = HumanPartId::iter().map(|id|
        {
            let location = UiAnatomyLocation::from_color(
                &mut part_creator,
                &base_image,
                color_pairs.iter().find(|(this_id, _)| *this_id == id).unwrap().1
            );

            (id, location)
        }).collect();

        Self{locations}
    }
}
