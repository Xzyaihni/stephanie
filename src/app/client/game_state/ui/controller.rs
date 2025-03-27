use std::{
    mem,
    collections::HashMap,
    hash::Hash,
    rc::Rc,
    sync::Arc
};

use nalgebra::{Vector2, Vector3};

use parking_lot::Mutex;

use yanyaengine::{
    Transform,
    FontsContainer,
    Assets,
    TextObject,
    TextInfo,
    game_object::*
};

use crate::{
    client::RenderCreateInfo,
    common::render_info::*
};

use super::element::*;


pub trait UiIdable
{
    fn screen() -> Self;
}

#[derive(Debug)]
struct UiElementCached
{
    object: Option<ClientRenderObject>
}

impl UiElementCached
{
    fn from_element(
        create_info: &mut RenderCreateInfo,
        deferred: UiDeferredInfo,
        element: &UiElement
    ) -> Self
    {
        let transform = Transform{
            scale: Vector3::new(
                deferred.width.unwrap(),
                deferred.height.unwrap(),
                1.0
            ),
            ..Default::default()
        };

        let object = match &element.texture
        {
            UiTexture::None => None,
            UiTexture::Text{text, font_size, font, align} =>
            {
                RenderObject{
                    kind: RenderObjectKind::Text{
                        text: text.clone(),
                        font_size: *font_size,
                        font: *font,
                        align: *align
                    }
                }.into_client(transform, create_info)
            },
            UiTexture::Solid
            | UiTexture::Custom(_) =>
            {
                RenderObject{
                    kind: RenderObjectKind::Texture{name: element.texture.name().unwrap().to_owned()}
                }.into_client(transform, create_info)
            }
        };

        Self{
            object
        }
    }
}

#[derive(Debug, Clone)]
pub struct UiDeferredInfo
{
    width: ResolvedSize,
    height: ResolvedSize
}

impl Default for UiDeferredInfo
{
    fn default() -> Self
    {
        Self{
            width: ResolvedSize::default(),
            height: ResolvedSize::default()
        }
    }
}

impl UiDeferredInfo
{
    fn screen() -> Self
    {
        let one = ResolvedSize{minimum_size: None, size: Some(1.0)};

        Self{
            width: one.clone(),
            height: one
        }
    }

    fn resolve_forward(&mut self, element: &UiElement, parent: &Self)
    {
        if !self.width.resolved()
        {
            self.width = element.width.resolve_forward(SizeForwardInfo{
                parent: parent.width.size
            });
        }

        if !self.height.resolved()
        {
            self.height = element.height.resolve_forward(SizeForwardInfo{
                parent: parent.height.size
            });
        }
    }

    fn resolve_backward(
        &mut self,
        sizer: &TextureSizer,
        element: &UiElement,
        children: Vec<ResolvedBackward>
    ) -> ResolvedBackward
    {
        let texture_size = || sizer.size(&element.texture);

        ResolvedBackward{
            width: self.width.resolve_backward(
                || texture_size().x,
                &element.width,
                children.iter().map(|x| x.width.clone())
            ),
            height: self.height.resolve_backward(
                || texture_size().y,
                &element.height,
                children.iter().map(|x| x.height.clone())
            )
        }
    }

    fn resolved(&self) -> bool
    {
        self.width.resolved() && self.height.resolved()
    }
}

#[derive(Debug)]
pub struct TreeElement<Id>
{
    element: UiElement,
    deferred: UiDeferredInfo,
    children: Vec<(Id, Self)>
}

impl<Id> TreeElement<Id>
{
    pub fn new(element: UiElement) -> Self
    {
        Self{
            element,
            deferred: UiDeferredInfo::default(),
            children: Vec::new()
        }
    }

    fn screen() -> Self
    {
        Self{
            element: UiElement{
                texture: UiTexture::None,
                ..Default::default()
            },
            deferred: UiDeferredInfo::screen(),
            children: Vec::new()
        }
    }

    pub fn update(&mut self, id: Id, element: UiElement) -> &mut Self
    {
        let index = self.children.len();
        self.children.push((id, Self::new(element)));

        &mut self.children[index].1
    }

    pub fn resolve_backward(&mut self, sizer: &TextureSizer) -> ResolvedBackward
    {
        let infos: Vec<_> = self.children.iter_mut().map(|(_, x)| x.resolve_backward(sizer)).collect();

        self.deferred.resolve_backward(sizer, &self.element, infos)
    }

    pub fn resolve_forward(&mut self, parent: &UiDeferredInfo)
    {
        if !self.deferred.resolved()
        {
            self.deferred.resolve_forward(&self.element, parent);
        }

        self.children.iter_mut().for_each(|(_, x)| x.resolve_forward(&self.deferred));
    }

    pub fn resolved(&self) -> bool
    {
        self.deferred.resolved() && self.children.iter().all(|(_, x)| x.resolved())
    }

    fn for_each(self, id: Id, mut f: impl FnMut(Id, UiElement, UiDeferredInfo))
    {
        self.for_each_inner(id, &mut f)
    }

    fn for_each_inner(self, id: Id, f: &mut impl FnMut(Id, UiElement, UiDeferredInfo))
    {
        f(id, self.element, self.deferred);
        self.children.into_iter().for_each(|(id, child)| child.for_each_inner(id, f));
    }
}

pub struct TextureSizer
{
    fonts: Rc<FontsContainer>,
    assets: Arc<Mutex<Assets>>
}

impl TextureSizer
{
    pub fn new(info: &ObjectCreatePartialInfo) -> Self
    {
        Self{
            fonts: info.builder_wrapper.fonts().clone(),
            assets: info.assets.clone()
        }
    }

    pub fn size(&self, texture: &UiTexture) -> Vector2<f32>
    {
        match texture
        {
            UiTexture::None => Vector2::zeros(),
            UiTexture::Text{text, font_size, font, align} =>
            {
                TextObject::calculate_bounds(TextInfo{
                    font_size: *font_size,
                    font: *font,
                    align: *align,
                    text
                }, &self.fonts)
            },
            UiTexture::Solid
            | UiTexture::Custom(_) =>
            {
                self.assets.lock().texture_by_name(texture.name().unwrap()).read().size()
            }
        }
    }
}

pub struct Controller<Id>
{
    sizer: TextureSizer,
    order: Vec<Id>,
    created: Vec<(Id, UiElement, UiDeferredInfo)>,
    elements: HashMap<Id, (UiElement, UiElementCached)>,
    root: TreeElement<Id>
}

impl<Id: Hash + Eq + Clone + UiIdable> Controller<Id>
{
    pub fn new(info: &ObjectCreatePartialInfo) -> Self
    {
        Self{
            sizer: TextureSizer::new(info),
            order: Vec::new(),
            created: Vec::new(),
            elements: HashMap::new(),
            root: TreeElement::screen()
        }
    }

    pub fn begin(&mut self)
    {
        self.order.clear();
    }

    pub fn update(&mut self, id: Id, element: UiElement) -> &mut TreeElement<Id>
    {
        debug_assert!(!self.order.contains(&id));

        self.root.update(id, element)
    }

    fn prepare(&mut self)
    {
        let empty = UiDeferredInfo::default();

        const LIMIT: usize = 1000;
        for i in 0..LIMIT
        {
            self.root.resolve_forward(&empty);
            self.root.resolve_backward(&self.sizer);

            if self.root.resolved()
            {
                break;
            }

            if i == (LIMIT - 1)
            {
                panic!("must be resolved");
            }
        }

        mem::replace(&mut self.root, TreeElement::screen()).for_each(Id::screen(), |id, element, deferred|
        {
            self.order.push(id.clone());

            self.created.push((id, element, deferred));
        });
    }

    pub fn create_renders(
        &mut self,
        create_info: &mut RenderCreateInfo
    )
    {
        self.prepare();

        mem::take(&mut self.created).into_iter().for_each(|(id, element, deferred)|
        {
            let element_cached = UiElementCached::from_element(create_info, deferred, &element);
            if let Some((old_element, old_element_cached)) = self.elements.get_mut(&id)
            {
                // do fancy ops here later
                *old_element = element;
                *old_element_cached = element_cached;
            } else
            {
                self.elements.insert(id, (element, element_cached));
            }
        });
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.elements.values_mut().for_each(|(_, element)|
        {
            if let Some(object) = element.object.as_mut()
            {
                object.update_buffers(info)
            }
        });
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo
    )
    {
        self.order.iter().for_each(|id|
        {
            let (element, element_cached) = &self.elements[id];
            if let Some(object) = element_cached.object.as_ref()
            {
                info.push_constants(UiOutlinedInfo::new(element.mix));

                object.draw(info)
            }
        });
    }
}
