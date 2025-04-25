use std::{
    mem,
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
    some_or_value,
    client::RenderCreateInfo,
    common::render_info::*
};

use super::element::*;


pub const MINIMUM_SCALE: f32 = 0.001;

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
        let scaling = element.animation.scaling
            .as_ref()
            .map(|x| x.start_scaling)
            .unwrap_or(Vector2::repeat(1.0));

        let width = deferred.width.unwrap() * scaling.x;
        let height = deferred.height.unwrap() * scaling.y;

        let position = deferred.position.unwrap();

        let transform = Transform{
            scale: Vector3::new(width, height, 1.0),
            position: Vector3::new(position.x, position.y, 0.0),
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

    fn update(
        &mut self,
        create_info: &mut RenderCreateInfo,
        deferred: UiDeferredInfo,
        element: &mut UiElement,
        dt: f32
    )
    {
        if let Some(object) = self.object.as_mut()
        {
            let mut transform = object.transform().cloned().unwrap_or_default();

            let position = deferred.position.unwrap();
            transform.position = Vector3::new(position.x, position.y, 0.0);

            let target_scale = Vector3::new(deferred.width.unwrap(), deferred.height.unwrap(), 1.0);

            if let Some(scaling) = element.animation.scaling.as_mut()
            {
                scaling.start_mode.next(&mut transform.scale, target_scale, dt);
            } else
            {
                transform.scale = target_scale;
            }

            object.set_transform(transform);
        } else
        {
            self.object = Self::from_element(create_info, deferred, element).object;
        }
    }

    fn update_closing(&mut self, element: &mut UiElement, dt: f32) -> bool
    {
        let object = some_or_value!(self.object.as_mut(), false);
        let mut transform = some_or_value!(object.transform().cloned(), false);

        let scaling = some_or_value!(element.animation.scaling.as_mut(), false);

        if let Scaling::Ignore = scaling.close_mode
        {
            return false;
        }

        scaling.close_mode.next(&mut transform.scale, Vector3::zeros(), dt);

        if transform.scale.max() < MINIMUM_SCALE
        {
            return false;
        }

        object.set_transform(transform);

        true
    }
}

#[derive(Debug, Clone)]
pub struct UiDeferredInfo
{
    position: Option<Vector2<f32>>,
    width: ResolvedSize,
    height: ResolvedSize
}

impl Default for UiDeferredInfo
{
    fn default() -> Self
    {
        Self{
            position: None,
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
            position: Some(Vector2::zeros()),
            width: one.clone(),
            height: one
        }
    }

    fn resolve_forward(
        &mut self,
        element: &UiElement,
        previous: Option<&Self>,
        parent: &Self,
        parent_element: &UiElement
    )
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

        if self.position.is_none()
        {
            if let UiPosition::Absolute(x) = element.position
            {
                self.position = Some(x);
            } else if let Some(previous) = previous
            {
                if let Some(previous_position) = previous.position
                {
                    self.position = element.position.resolve_forward(
                        &parent_element.children_layout,
                        previous_position,
                        self.width.value(),
                        self.height.value()
                    );
                }
            } else
            {
                self.position = self.starting_position(parent);
            }
        }
    }

    fn starting_position(&self, parent: &Self) -> Option<Vector2<f32>>
    {
        let this_size = Vector2::new(self.width.value()?, self.height.value()?);
        let parent_size = Vector2::new(parent.width.value()?, parent.height.value()?);

        Some(parent.position? + (this_size - parent_size) / 2.0)
    }

    fn resolve_backward(
        &mut self,
        sizer: &TextureSizer,
        element: &UiElement,
        children: Vec<ResolvedBackward>
    ) -> ResolvedBackward
    {
        let texture_size = || sizer.size(&element.texture);

        let is_width_parallel = element.children_layout.is_horizontal();

        ResolvedBackward{
            width: self.width.resolve_backward(
                || texture_size().x,
                is_width_parallel,
                &element.width,
                children.iter().map(|x| x.width.clone())
            ),
            height: self.height.resolve_backward(
                || texture_size().y,
                !is_width_parallel,
                &element.height,
                children.iter().map(|x| x.height.clone())
            )
        }
    }

    fn resolved(&self) -> bool
    {
        self.width.resolved()
            && self.height.resolved()
            && self.position.is_some()
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
            ..Self::screen()
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

    pub fn resolve_forward(
        &mut self,
        previous: Option<&UiDeferredInfo>,
        parent: &UiDeferredInfo,
        parent_element: &UiElement
    )
    {
        if !self.deferred.resolved()
        {
            self.deferred.resolve_forward(
                &self.element,
                previous,
                parent,
                parent_element
            );
        }

        self.children.iter_mut().fold(None, |previous, (_, x)|
        {
            x.resolve_forward(previous, &self.deferred, &self.element);

            Some(&x.deferred)
        });
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
    assets: Arc<Mutex<Assets>>,
    size: Vector2<f32>
}

impl TextureSizer
{
    pub fn new(info: &ObjectCreatePartialInfo) -> Self
    {
        Self{
            fonts: info.builder_wrapper.fonts().clone(),
            assets: info.assets.clone(),
            size: info.size.into()
        }
    }

    pub fn update_screen_size(&mut self, size: Vector2<f32>)
    {
        self.size = size;
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
                }, &self.fonts, &self.size).component_mul(&(self.size / self.size.max()))
            },
            UiTexture::Solid
            | UiTexture::Custom(_) =>
            {
                self.assets.lock().texture_by_name(texture.name().unwrap()).read().size() / self.size.max()
            }
        }
    }
}

#[derive(Debug)]
struct Element<Id>
{
    id: Id,
    element: UiElement,
    cached: UiElementCached,
    closing: bool
}

pub struct Controller<Id>
{
    sizer: TextureSizer,
    created: Vec<(Id, UiElement, UiDeferredInfo)>,
    elements: Vec<Element<Id>>,
    root: TreeElement<Id>
}

impl<Id: Hash + Eq + Clone + UiIdable> Controller<Id>
{
    pub fn new(info: &ObjectCreatePartialInfo) -> Self
    {
        Self{
            sizer: TextureSizer::new(info),
            created: Vec::new(),
            elements: Vec::new(),
            root: TreeElement::screen()
        }
    }

    pub fn update(&mut self, id: Id, element: UiElement) -> &mut TreeElement<Id>
    {
        debug_assert!(!self.created.iter().any(|(x, _, _)| *x == id));

        self.root.update(id, element)
    }

    fn prepare(&mut self)
    {
        let empty = UiDeferredInfo::default();
        let empty_element = UiElement::default();

        const LIMIT: usize = 1000;
        for i in 0..LIMIT
        {
            self.root.resolve_forward(None, &empty, &empty_element);
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
            self.created.push((id, element, deferred));
        });
    }

    pub fn create_renders(
        &mut self,
        create_info: &mut RenderCreateInfo,
        dt: f32
    )
    {
        self.prepare();

        self.elements.iter_mut().for_each(|element| element.closing = true);
        mem::take(&mut self.created).into_iter().for_each(|(id, element, deferred)|
        {
            if let Some(index) = self.elements.iter().position(|element| element.id == id)
            {
                let Element{element: old_element, cached: old_cached, closing, ..} = &mut self.elements[index];

                *closing = false;

                if *old_element == element
                {
                    old_cached.update(create_info, deferred, old_element, dt);
                } else
                {
                    *old_cached = UiElementCached::from_element(create_info, deferred, &element);
                    *old_element = element;
                }
            } else
            {
                let cached = UiElementCached::from_element(create_info, deferred, &element);
                self.elements.push(Element{id, element, cached, closing: false});
            }
        });

        self.elements.retain_mut(|element|
        {
            if element.closing
            {
                element.cached.update_closing(&mut element.element, dt)
            } else
            {
                true
            }
        });
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.sizer.update_screen_size(info.partial.size.into());

        self.elements.iter_mut().for_each(|Element{cached, ..}|
        {
            if let Some(object) = cached.object.as_mut()
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
        self.elements.iter().for_each(|Element{element, cached, ..}|
        {
            if let Some(object) = cached.object.as_ref()
            {
                info.push_constants(UiOutlinedInfo::new(element.mix));

                object.draw(info)
            }
        });
    }
}
