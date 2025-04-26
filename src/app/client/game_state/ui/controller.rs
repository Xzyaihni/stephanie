use std::{
    mem,
    hash::Hash,
    rc::Rc,
    cell::RefCell,
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

pub trait Idable: Hash + Eq + Clone
{
    fn screen() -> Self;
    fn padding(id: u32) -> Self;
}

pub trait TreeElementable<Id: Idable>
{
    fn update(&mut self, id: Id, element: UiElement) -> &mut TreeElement<Id>;
    fn consecutive(&mut self) -> u32;
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
        deferred: &UiDeferredInfo,
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
        deferred: &UiDeferredInfo,
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
            self.object = Self::from_element(create_info, &deferred, element).object;
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
                if let (Some(previous_position), Some(parent_position)) = (previous.position, parent.position)
                {
                    self.position = Some(element.position.resolve_forward(
                        &parent_element.children_layout,
                        previous_position,
                        PositionResolveInfo{
                            this: self.width.unwrap(),
                            previous: previous.width.unwrap(),
                            parent_position: parent_position.x
                        },
                        PositionResolveInfo{
                            this: self.height.unwrap(),
                            previous: previous.height.unwrap(),
                            parent_position: parent_position.y
                        }
                    ));
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
    id: Id,
    element: UiElement,
    deferred: UiDeferredInfo,
    children: Vec<(Id, Self)>,
    shared: Rc<RefCell<SharedInfo<Id>>>
}

impl<Id> TreeElement<Id>
{
    fn new(shared: Rc<RefCell<SharedInfo<Id>>>, id: Id, element: UiElement) -> Self
    {
        Self{
            id,
            element,
            deferred: UiDeferredInfo::default(),
            children: Vec::new(),
            shared
        }
    }

    fn screen(shared: Rc<RefCell<SharedInfo<Id>>>) -> Self
    where
        Id: Idable
    {
        Self{
            id: Id::screen(),
            element: UiElement::default(),
            deferred: UiDeferredInfo::screen(),
            children: Vec::new(),
            shared
        }
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

    pub fn is_inside(&self, check_position: Vector2<f32>) -> bool
    where
        Id: Eq
    {
        let shared = self.shared.borrow();
        shared.element_id(&self.id).map(|index|
        {
            let deferred = &shared.elements[index].deferred;

            let position = deferred.position.unwrap();
            let size = Vector2::new(deferred.width.unwrap(), deferred.height.unwrap());

            let checks = (check_position - position).zip_map(&size, |x, size|
            {
                let half_size = size / 2.0;
                (-half_size..=half_size).contains(&x)
            });

            checks.x && checks.y
        }).unwrap_or(false)
    }

    pub fn is_mouse_inside(&self) -> bool
    where
        Id: Eq
    {
        self.is_inside(self.shared.borrow().mouse_position)
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

impl<Id: Idable> TreeElementable<Id> for TreeElement<Id>
{
    fn update(&mut self, id: Id, element: UiElement) -> &mut Self
    {
        let index = self.children.len();
        self.children.push((id.clone(), Self::new(self.shared.clone(), id, element)));

        &mut self.children[index].1
    }

    fn consecutive(&mut self) -> u32
    {
        let consecutive = &mut self.shared.borrow_mut().consecutive;
        let x = *consecutive;

        *consecutive += 1;

        x
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
    deferred: UiDeferredInfo,
    closing: bool
}

#[derive(Debug)]
struct SharedInfo<Id>
{
    consecutive: u32,
    mouse_position: Vector2<f32>,
    elements: Vec<Element<Id>>
}

impl<Id> SharedInfo<Id>
{
    pub fn new() -> Self
    {
        Self{
            consecutive: 0,
            mouse_position: Vector2::zeros(),
            elements: Vec::new()
        }
    }

    pub fn element_id(&self, id: &Id) -> Option<usize>
    where
        Id: Eq
    {
        self.elements.iter().position(|element| element.id == *id)
    }
}

pub struct Controller<Id>
{
    sizer: TextureSizer,
    created: Vec<(Id, UiElement, UiDeferredInfo)>,
    root: TreeElement<Id>,
    shared: Rc<RefCell<SharedInfo<Id>>>
}

impl<Id: Idable> Controller<Id>
{
    pub fn new(info: &ObjectCreatePartialInfo) -> Self
    {
        let shared = Rc::new(RefCell::new(SharedInfo::new()));

        Self{
            sizer: TextureSizer::new(info),
            created: Vec::new(),
            root: TreeElement::screen(shared.clone()),
            shared
        }
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

        mem::replace(&mut self.root, TreeElement::screen(self.shared.clone()))
            .for_each(Id::screen(), |id, element, deferred|
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
        self.shared.borrow_mut().consecutive = 0;

        self.prepare();

        self.shared.borrow_mut().elements.iter_mut().for_each(|element| element.closing = true);
        mem::take(&mut self.created).into_iter().for_each(|(id, element, deferred)|
        {
            let index = self.shared.borrow().element_id(&id);
            if let Some(index) = index
            {
                let Element{
                    element: old_element,
                    cached: old_cached,
                    closing,
                    ..
                } = &mut self.shared.borrow_mut().elements[index];

                *closing = false;

                if *old_element == element
                {
                    old_cached.update(create_info, &deferred, old_element, dt);
                } else
                {
                    *old_cached = UiElementCached::from_element(create_info, &deferred, &element);
                    *old_element = element;
                }
            } else
            {
                let cached = UiElementCached::from_element(create_info, &deferred, &element);
                self.shared.borrow_mut().elements.push(Element{id, element, cached, deferred, closing: false});
            }
        });

        self.shared.borrow_mut().elements.retain_mut(|element|
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

    pub fn set_mouse_position(&mut self, position: Vector2<f32>)
    {
        self.shared.borrow_mut().mouse_position = position;
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.sizer.update_screen_size(info.partial.size.into());

        self.shared.borrow_mut().elements.iter_mut().for_each(|Element{cached, ..}|
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
        self.shared.borrow().elements.iter().for_each(|Element{element, cached, ..}|
        {
            if let Some(object) = cached.object.as_ref()
            {
                info.push_constants(UiOutlinedInfo::new(element.mix));

                object.draw(info)
            }
        });
    }
}

impl<Id: Idable> TreeElementable<Id> for Controller<Id>
{
    fn update(&mut self, id: Id, element: UiElement) -> &mut TreeElement<Id>
    {
        debug_assert!(!self.created.iter().any(|(x, _, _)| *x == id));

        self.root.update(id, element)
    }

    fn consecutive(&mut self) -> u32
    {
        self.root.consecutive()
    }
}

pub fn add_padding<E: TreeElementable<Id>, Id: Idable>(x: &mut E, width: UiElementSize, height: UiElementSize)
{
    let id = x.consecutive();
    x.update(Id::padding(id), UiElement{
        width,
        height,
        ..Default::default()
    });
}

pub fn add_padding_horizontal<E: TreeElementable<Id>, Id: Idable>(x: &mut E, size: UiElementSize)
{
    add_padding(x, size, 0.0.into())
}

pub fn add_padding_vertical<E: TreeElementable<Id>, Id: Idable>(x: &mut E, size: UiElementSize)
{
    add_padding(x, 0.0.into(), size)
}
