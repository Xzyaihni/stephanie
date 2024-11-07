use std::sync::Arc;

use vulkano::{
    format::Format,
    memory::allocator::{
        StandardMemoryAllocator,
        AllocationCreateInfo
    },
    image::{
        Image,
        ImageCreateInfo,
        ImageType,
        ImageUsage,
        view::ImageView
    }
};

use yanyaengine::Rendering;


pub fn create() -> Rendering
{
    Rendering{
        render_pass: Box::new(|device, image_format|
        {
            vulkano::single_pass_renderpass!(
                device,
                attachments: {
                    color: {
                        format: image_format,
                        samples: 1,
                        load_op: Clear,
                        store_op: Store
                    },
                    depth: {
                        format: Format::D16_UNORM,
                        samples: 1,
                        load_op: Clear,
                        store_op: DontCare
                    }
                },
                pass: {
                    color: [color],
                    depth_stencil: {depth}
                }
            ).unwrap()
        }),
        attachments: Box::new(|allocator: Arc<StandardMemoryAllocator>, view: Arc<ImageView>|
        {
            let depth_image = Image::new(
                allocator,
                ImageCreateInfo{
                    image_type: ImageType::Dim2d,
                    format: Format::D16_UNORM,
                    extent: view.image().extent(),
                    usage: ImageUsage::TRANSIENT_ATTACHMENT | ImageUsage::DEPTH_STENCIL_ATTACHMENT,
                    ..Default::default()
                },
                AllocationCreateInfo::default()
            ).unwrap();

            let depth = ImageView::new_default(depth_image).unwrap();

            vec![view, depth]
        }),
        clear: vec![Some([0.831, 0.941, 0.988, 1.0].into()), Some(1.0.into())]
    }
}
