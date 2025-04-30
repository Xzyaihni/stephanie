use std::{
    mem,
    fmt,
    hash::Hash,
    rc::Rc,
    cell::RefCell,
    sync::Arc,
    collections::HashMap,
    fmt::Debug
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
    common::{
        render_info::*,
        LazyMix
    }
};

use super::element::*;


pub const MINIMUM_SCALE: f32 = 0.001;

pub trait Idable: Hash + Eq + Clone + Debug
{
    fn screen() -> Self;
    fn padding(id: u32) -> Self;
}

pub trait TreeElementable<Id: Idable>
{
    fn update(&mut self, id: Id, element: UiElement<Id>) -> &mut TreeElement<Id>;
    fn consecutive(&mut self) -> u32;
}

#[derive(Debug)]
struct UiElementCached
{
    mix: Option<MixColor>,
    object: Option<ClientRenderObject>
}

impl UiElementCached
{
    fn from_element<Id>(
        create_info: &mut RenderCreateInfo,
        deferred: &UiDeferredInfo,
        element: &UiElement<Id>
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
            mix: element.mix,
            object
        }
    }

    fn update<Id>(
        &mut self,
        create_info: &mut RenderCreateInfo,
        deferred: &UiDeferredInfo,
        old_element: &mut UiElement<Id>,
        dt: f32
    )
    {
        if let (
            Some(decay),
            Some(mix),
            Some(target)
        ) = (old_element.animation.mix, self.mix.as_mut(), old_element.mix)
        {
            *mix = LazyMix{decay, target}.update(*mix, dt);
        }

        if let Some(object) = self.object.as_mut()
        {
            let mut transform = object.transform().cloned().unwrap_or_default();

            let position = deferred.position.unwrap();
            transform.position = Vector3::new(position.x, position.y, 0.0);

            let target_scale = Vector3::new(deferred.width.unwrap(), deferred.height.unwrap(), 1.0);

            if let Some(scaling) = old_element.animation.scaling.as_mut()
            {
                scaling.start_mode.next(&mut transform.scale, target_scale, dt);
            } else
            {
                transform.scale = target_scale;
            }

            object.set_transform(transform);
        } else
        {
            self.object = Self::from_element(create_info, &deferred, old_element).object;
        }
    }

    fn update_closing<Id>(&mut self, element: &mut UiElement<Id>, dt: f32) -> bool
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

    fn keep_old<Id: Eq>(&self, new: &mut Self, old_element: &UiElement<Id>, new_element: &UiElement<Id>)
    {
        macro_rules! fields_match
        {
            ($($field:ident),+) =>
            {
                true $(&& old_element.$field == new_element.$field)+
            }
        }

        if fields_match!(texture, animation, position, children_layout, width, height)
        {
            debug_assert!(old_element.mix != new_element.mix);

            new.mix = self.mix;

            if let (Some(new), Some(old)) = (new.object.as_mut(), self.object.as_ref().and_then(|x| x.transform()))
            {
                new.modify_transform(|transform| transform.scale = old.scale);
            }
        }
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

    fn resolve_forward<Id: Idable>(
        &mut self,
        resolved: &HashMap<Id, Self>,
        element: &UiElement<Id>,
        previous: Option<&Self>,
        parent: &Self,
        parent_element: &UiElement<Id>
    )
    {
        let get_element_size = |direction: &_, id: &Id| -> Option<f32>
        {
            let element = resolved.get(id)?;

            let size = match direction
            {
                UiDirection::Horizontal => element.width,
                UiDirection::Vertical => element.height
            };

            size.value()
        };

        if !self.width.resolved()
        {
            self.width = element.width.resolve_forward(SizeForwardInfo{
                parent: parent.width.size,
                get_element_size
            });
        }

        if !self.height.resolved()
        {
            self.height = element.height.resolve_forward(SizeForwardInfo{
                parent: parent.height.size,
                get_element_size
            });
        }

        if self.position.is_none()
        {
            match &element.position
            {
                UiPosition::Absolute(x) => self.position = Some(*x),
                UiPosition::Offset(id, x) =>
                {
                    self.position = resolved.get(id).and_then(|element| element.position.map(|pos| pos + *x));
                },
                _ =>
                {
                    if let Some(previous) = previous
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
        }
    }

    fn starting_position(&self, parent: &Self) -> Option<Vector2<f32>>
    {
        let this_size = Vector2::new(self.width.value()?, self.height.value()?);
        let parent_size = Vector2::new(parent.width.value()?, parent.height.value()?);

        Some(parent.position? + (this_size - parent_size) / 2.0)
    }

    fn resolve_backward<Id: Idable>(
        &mut self,
        sizer: &TextureSizer,
        element: &UiElement<Id>,
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

    fn resolve_children<Id>(&self, children: &mut [TreeElement<Id>])
    {
        self.width.resolve_children(children.iter_mut().map(|x| (&mut x.deferred.width.size, &x.element.width.size)));
        self.height.resolve_children(children.iter_mut().map(|x| (&mut x.deferred.height.size, &x.element.height.size)));
    }

    fn resolved(&self) -> bool
    {
        self.width.resolved()
            && self.height.resolved()
            && self.position.is_some()
    }
}

pub struct TreeElement<Id>
{
    id: Id,
    element: UiElement<Id>,
    deferred: UiDeferredInfo,
    children: Vec<Self>,
    shared: Rc<RefCell<SharedInfo<Id>>>
}

impl<Id: Debug> Debug for TreeElement<Id>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        f.debug_struct("TreeElement")
            .field("id", &self.id)
            .field("element", &self.element)
            .field("deferred", &self.deferred)
            .field("children", &self.children)
            .finish()
    }
}

impl<Id: Idable> TreeElement<Id>
{
    fn new(shared: Rc<RefCell<SharedInfo<Id>>>, id: Id, element: UiElement<Id>) -> Self
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
        let infos: Vec<_> = self.children.iter_mut().map(|x| x.resolve_backward(sizer)).collect();

        let resolved = self.deferred.resolve_backward(sizer, &self.element, infos);
        self.deferred.resolve_children(&mut self.children);

        resolved
    }

    pub fn resolve_forward(
        &mut self,
        resolved: &mut HashMap<Id, UiDeferredInfo>,
        previous: Option<&UiDeferredInfo>,
        parent: &UiDeferredInfo,
        parent_element: &UiElement<Id>
    )
    where
        Id: Idable
    {
        if !self.deferred.resolved()
        {
            self.deferred.resolve_forward(
                resolved,
                &self.element,
                previous,
                parent,
                parent_element
            );

            resolved.insert(self.id.clone(), self.deferred.clone());
        }

        self.children.iter_mut().fold(None, |previous, x|
        {
            x.resolve_forward(resolved, previous, &self.deferred, &self.element);

            Some(&x.deferred)
        });
    }

    pub fn resolved(&self) -> bool
    {
        self.deferred.resolved() && self.children.iter().all(|x| x.resolved())
    }

    pub fn element(&mut self) -> &mut UiElement<Id>
    {
        &mut self.element
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

    fn for_each(self, mut f: impl FnMut(Id, UiElement<Id>, UiDeferredInfo))
    {
        self.for_each_inner(&mut f)
    }

    fn for_each_inner(self, f: &mut impl FnMut(Id, UiElement<Id>, UiDeferredInfo))
    {
        f(self.id, self.element, self.deferred);
        self.children.into_iter().for_each(|child| child.for_each_inner(f));
    }
}

impl<Id: Idable> TreeElementable<Id> for TreeElement<Id>
{
    fn update(&mut self, id: Id, element: UiElement<Id>) -> &mut Self
    {
        let index = self.children.len();
        self.children.push(Self::new(self.shared.clone(), id, element));

        &mut self.children[index]
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
    element: UiElement<Id>,
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

impl<Id: Idable> SharedInfo<Id>
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
    {
        self.elements.iter().position(|element| element.id == *id)
    }
}

pub struct Controller<Id>
{
    sizer: TextureSizer,
    created: Vec<(Id, UiElement<Id>, UiDeferredInfo)>,
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
        let mut resolved = HashMap::new();
        let empty = UiDeferredInfo::default();
        let empty_element = UiElement::default();

        const LIMIT: usize = 1000;
        for i in 0..LIMIT
        {
            self.root.resolve_forward(&mut resolved, None, &empty, &empty_element);
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
            .for_each(|id, element, deferred|
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

        let mut last_match = None;
        mem::take(&mut self.created).into_iter().for_each(|(id, element, deferred)|
        {
            let index = self.shared.borrow().element_id(&id);
            if let Some(index) = index
            {
                last_match = Some(index);

                let Element{
                    element: old_element,
                    cached: old_cached,
                    deferred: old_deferred,
                    closing,
                    ..
                } = &mut self.shared.borrow_mut().elements[index];

                *closing = false;

                if *old_element == element
                {
                    old_cached.update(create_info, &deferred, old_element, dt);
                } else
                {
                    let mut cached = UiElementCached::from_element(create_info, &deferred, &element);

                    old_cached.keep_old(&mut cached, old_element, &element);

                    *old_cached = cached;

                    *old_element = element;
                }

                *old_deferred = deferred;
            } else
            {
                let cached = UiElementCached::from_element(create_info, &deferred, &element);
                let element = Element{id, element, cached, deferred, closing: false};

                let elements = &mut self.shared.borrow_mut().elements;

                if let Some(index) = last_match
                {
                    elements.insert(index + 1, element);

                    last_match = Some(index + 1);
                } else
                {
                    elements.push(element);
                }
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
        self.shared.borrow().elements.iter().for_each(|Element{cached, ..}|
        {
            if let Some(object) = cached.object.as_ref()
            {
                info.push_constants(UiOutlinedInfo::new(cached.mix));

                object.draw(info)
            }
        });
    }
}

impl<Id: Idable> TreeElementable<Id> for Controller<Id>
{
    fn update(&mut self, id: Id, element: UiElement<Id>) -> &mut TreeElement<Id>
    {
        debug_assert!(!self.created.iter().any(|(x, _, _)| *x == id));

        self.root.update(id, element)
    }

    fn consecutive(&mut self) -> u32
    {
        self.root.consecutive()
    }
}

pub fn add_padding<E: TreeElementable<Id>, Id: Idable>(x: &mut E, width: UiElementSize<Id>, height: UiElementSize<Id>)
{
    let id = x.consecutive();
    x.update(Id::padding(id), UiElement{
        width,
        height,
        ..Default::default()
    });
}

pub fn add_padding_horizontal<E: TreeElementable<Id>, Id: Idable>(x: &mut E, size: UiElementSize<Id>)
{
    add_padding(x, size, 0.0.into())
}

pub fn add_padding_vertical<E: TreeElementable<Id>, Id: Idable>(x: &mut E, size: UiElementSize<Id>)
{
    add_padding(x, 0.0.into(), size)
}
