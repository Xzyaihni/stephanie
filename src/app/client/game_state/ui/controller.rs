use std::{
    fmt,
    hash::Hash,
    rc::Rc,
    cell::{Ref, RefMut, RefCell},
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
    some_or_return,
    client::RenderCreateInfo,
    common::{
        render_info::*,
        LazyMix
    }
};

use super::element::*;


pub const MINIMUM_SCALE: f32 = 0.0005;

pub trait Idable: Hash + Eq + Clone + Debug
{
    fn screen() -> Self;
    fn padding(id: u32) -> Self;
}

#[derive(Debug, Clone, Copy)]
pub struct TreeInserter<'a, Id>
{
    elements: &'a RefCell<Vec<TreeElement<Id>>>,
    index: usize
}

impl<'a, Id: Idable> TreeInserter<'a, Id>
{
    fn tree_element(&self) -> Ref<TreeElement<Id>>
    {
        Ref::map(self.elements.borrow(), |x| &x[self.index])
    }

    fn tree_element_mut(&self) -> RefMut<TreeElement<Id>>
    {
        RefMut::map(self.elements.borrow_mut(), |x| &mut x[self.index])
    }

    pub fn update(&self, id: Id, element: UiElement<Id>) -> TreeInserter<'a, Id>
    {
        debug_assert!(!self.elements.borrow().iter().any(|x| x.id == id));

        let shared = self.tree_element().shared.clone();
        let element = TreeElement::new(shared, id, element);

        let index = {
            let mut elements = self.elements.borrow_mut();

            let index = elements.len();
            elements.push(element);

            index
        };

        self.tree_element_mut().children.push(index);

        Self{
            elements: self.elements,
            index
        }
    }

    pub fn consecutive(&self) -> u32
    {
        self.tree_element_mut().consecutive()
    }

    pub fn element(&self) -> RefMut<UiElement<Id>>
    {
        RefMut::map(self.tree_element_mut(), |x| x.element())
    }

    pub fn is_inside(&self, check_position: Vector2<f32>) -> bool
    {
        self.tree_element().is_inside(check_position)
    }

    pub fn is_mouse_inside(&self) -> bool
    {
        self.tree_element().is_mouse_inside()
    }
}

#[derive(Debug)]
struct UiElementCached
{
    mix: Option<MixColor>,
    scissor: Option<VulkanoScissor>,
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
        let scissor = deferred.scissor.map(|x|
        {
            let highest = Vector2::from(create_info.object_info.partial.size).max();
            x.into_global([highest, highest])
        });

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
            UiTexture::Text{text, font_size} =>
            {
                RenderObject{
                    kind: RenderObjectKind::Text{
                        text: text.clone(),
                        font_size: *font_size
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
            scissor,
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

        if transform.scale.min() < MINIMUM_SCALE
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
    scissor: Option<Scissor>,
    scissor_resolved: bool,
    position: Option<Vector2<f32>>,
    width: ResolvedSize,
    height: ResolvedSize
}

impl Default for UiDeferredInfo
{
    fn default() -> Self
    {
        Self{
            scissor: None,
            scissor_resolved: false,
            position: None,
            width: ResolvedSize::default(),
            height: ResolvedSize::default()
        }
    }
}

impl UiDeferredInfo
{
    fn screen(aspect: Vector2<f32>) -> Self
    {
        let one = |v| ResolvedSize{minimum_size: None, size: Some(v)};

        Self{
            scissor: None,
            scissor_resolved: true,
            position: Some(Vector2::zeros()),
            width: one(aspect.x),
            height: one(aspect.y)
        }
    }

    fn resolve_forward<Id: Idable>(
        &mut self,
        resolved: &HashMap<Id, Self>,
        screen_size: &Vector2<f32>,
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

        if !self.scissor_resolved
        {
            if parent_element.scissor
            {
                debug_assert!(!element.scissor, "nested scissors not supported");

                if let (
                    Some(width),
                    Some(height),
                    Some(position)
                ) = (parent.width.value(), parent.height.value(), parent.position)
                {
                    let aspect = screen_size / screen_size.max();
                    let size = Vector2::new(width, height);
                    let offset = position + (aspect / 2.0) - (size / 2.0);

                    self.scissor = Some(Scissor{offset: offset.into(), extent: size.into()});
                }

                self.scissor_resolved = self.scissor.is_some();
            } else if let Some(scissor) = parent.scissor
            {
                debug_assert!(!element.scissor, "nested scissors not supported");

                self.scissor = Some(scissor);
                self.scissor_resolved = true;
            } else if parent.scissor_resolved
            {
                self.scissor_resolved = true;
            }
        }

        if !self.width.resolved()
        {
            self.width = element.width.resolve_forward(SizeForwardInfo{
                parent: parent.width.size,
                screen_size: screen_size.max(),
                get_element_size
            });
        }

        if !self.height.resolved()
        {
            self.height = element.height.resolve_forward(SizeForwardInfo{
                parent: parent.height.size,
                screen_size: screen_size.max(),
                get_element_size
            });
        }

        if self.position.is_none()
        {
            self.position = self.resolve_position(
                resolved,
                element,
                previous,
                parent,
                parent_element
            );
        }
    }

    fn resolve_position<Id: Idable>(
        &mut self,
        resolved: &HashMap<Id, Self>,
        element: &UiElement<Id>,
        previous: Option<&Self>,
        parent: &Self,
        parent_element: &UiElement<Id>
    ) -> Option<Vector2<f32>>
    {
        match &element.position
        {
            UiPosition::Absolute(x) => Some(*x),
            UiPosition::Offset(id, x) =>
            {
                resolved.get(id).and_then(|element| element.position.map(|pos| pos + *x))
            },
            _ =>
            {
                let parent_position = parent.position?;
                if let Some(previous) = previous
                {
                    let previous_position = previous.position?;
                    Some(element.position.resolve_forward(
                        &parent_element.children_layout,
                        previous_position,
                        PositionResolveInfo{
                            this: self.width.value()?,
                            previous: previous.width.value()?,
                            parent_position: parent_position.x
                        },
                        PositionResolveInfo{
                            this: self.height.value()?,
                            previous: previous.height.value()?,
                            parent_position: parent_position.y
                        }
                    ))
                } else
                {
                    Some(UiPosition::<Id>::next_position(
                        &parent_element.children_layout,
                        parent_position,
                        PositionResolveInfo{
                            this: self.width.value()?,
                            previous: -parent.width.value()?,
                            parent_position: parent_position.x
                        },
                        PositionResolveInfo{
                            this: self.height.value()?,
                            previous: -parent.height.value()?,
                            parent_position: parent_position.y
                        }
                    ))
                }
            }
        }
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

    fn resolved(&self) -> bool
    {
        self.width.resolved()
            && self.height.resolved()
            && self.position.is_some()
            && self.scissor_resolved
    }
}

pub struct TreeElement<Id>
{
    id: Id,
    element: UiElement<Id>,
    deferred: UiDeferredInfo,
    children: Vec<usize>,
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
        let aspect = shared.borrow().aspect();
        Self{
            id: Id::screen(),
            element: UiElement::default(),
            deferred: UiDeferredInfo::screen(aspect),
            children: Vec::new(),
            shared
        }
    }

    pub fn resolve_backward(
        trees: &mut Vec<TreeElement<Id>>,
        index: usize,
        sizer: &TextureSizer
    ) -> ResolvedBackward
    {
        let infos = {
            let mut infos = Vec::new();

            let children_len = trees[index].children.len();

            for i in 0..children_len
            {
                let x = trees[index].children[i];

                infos.push(Self::resolve_backward(trees, x, sizer));
            }

            infos
        };

        let resolved = {
            let this = &mut trees[index];
            this.deferred.resolve_backward(sizer, &this.element, infos)
        };

        macro_rules! for_children
        {
            ($name:ident, $is_width:expr) =>
            {
                {
                    let this = &mut trees[index];
                    if let Some($name) = this.deferred.$name.value()
                    {
                        let parallel = $is_width ^ (!this.element.children_layout.is_horizontal());

                        let children: Vec<_> = this.children.iter().copied().collect();
                        ResolvedSize::resolve_rest(
                            trees,
                            parallel,
                            $name,
                            |trees, index| &mut trees[index].deferred.$name.size,
                            |trees, index| trees[index].deferred.$name.minimum_size,
                            |trees, index| &trees[index].element.$name.size,
                            children.into_iter()
                        );
                    }
                }
            }
        }

        for_children!(width, true);
        for_children!(height, false);

        resolved
    }

    pub fn resolve_forward(
        trees: &mut Vec<TreeElement<Id>>,
        index: usize,
        parent_index: Option<usize>,
        resolved: &mut HashMap<Id, UiDeferredInfo>,
        previous: Option<usize>
    )
    where
        Id: Idable
    {
        let is_resolved = trees[index].deferred.resolved();
        if !is_resolved
        {
            let parent_index = parent_index.unwrap();
            let (this, parent, previous) = if let Some(previous) = previous
            {
                let [this, parent, previous] = trees.get_disjoint_mut([index, parent_index, previous]).unwrap();

                (this, parent, Some(&previous.deferred))
            } else
            {
                let [this, parent] = trees.get_disjoint_mut([index, parent_index]).unwrap();

                (this, parent, None)
            };

            let shared = this.shared.borrow();

            this.deferred.resolve_forward(
                resolved,
                &shared.screen_size,
                &this.element,
                previous,
                &parent.deferred,
                &parent.element
            );

            resolved.insert(this.id.clone(), this.deferred.clone());
        }

        let children_len = trees[index].children.len();

        let mut previous = None;
        for i in 0..children_len
        {
            let x = trees[index].children[i];
            Self::resolve_forward(trees, x, Some(index), resolved, previous);

            previous = Some(x);
        }
    }

    fn resolved(&self, trees: &Vec<TreeElement<Id>>) -> bool
    {
        self.deferred.resolved() && self.children.iter().all(|index| trees[*index].resolved(trees))
    }

    fn element(&mut self) -> &mut UiElement<Id>
    {
        &mut self.element
    }

    fn is_inside(&self, check_position: Vector2<f32>) -> bool
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

    fn is_mouse_inside(&self) -> bool
    {
        self.is_inside(self.shared.borrow().mouse_position)
    }

    fn consecutive(&mut self) -> u32
    {
        let consecutive = &mut self.shared.borrow_mut().consecutive;
        let x = *consecutive;

        *consecutive += 1;

        x
    }

    fn for_each(
        &self,
        trees: &Vec<TreeElement<Id>>,
        mut f: impl FnMut(Id, UiElement<Id>, UiDeferredInfo)
    )
    {
        self.for_each_inner(trees, &mut f)
    }

    fn for_each_inner(
        &self,
        trees: &Vec<TreeElement<Id>>,
        f: &mut impl FnMut(Id, UiElement<Id>, UiDeferredInfo)
    )
    {
        f(self.id.clone(), self.element.clone(), self.deferred.clone());
        self.children.iter().for_each(|index|
        {
            trees[*index].for_each_inner(trees, f)
        });
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
            UiTexture::Text{text, font_size} =>
            {
                TextObject::calculate_bounds(TextInfo{
                    font_size: *font_size,
                    text
                }, self.fonts.default_font(), &self.size).component_mul(&(self.size / self.size.max()))
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
    screen_size: Vector2<f32>,
    elements: Vec<Element<Id>>
}

impl<Id: Idable> SharedInfo<Id>
{
    pub fn new() -> Self
    {
        Self{
            consecutive: 0,
            mouse_position: Vector2::zeros(),
            screen_size: Vector2::repeat(1.0),
            elements: Vec::new()
        }
    }

    pub fn aspect(&self) -> Vector2<f32>
    {
        self.screen_size / self.screen_size.max()
    }

    pub fn element_id(&self, id: &Id) -> Option<usize>
    {
        self.elements.iter().position(|element| element.id == *id)
    }
}

pub struct Controller<Id>
{
    sizer: TextureSizer,
    created_trees: RefCell<Vec<TreeElement<Id>>>,
    shared: Rc<RefCell<SharedInfo<Id>>>
}

impl<Id: Idable> Controller<Id>
{
    pub fn new(info: &ObjectCreatePartialInfo) -> Self
    {
        let shared = Rc::new(RefCell::new(SharedInfo::new()));
        Self{
            sizer: TextureSizer::new(info),
            created_trees: RefCell::new(vec![TreeElement::screen(shared.clone())]),
            shared
        }
    }

    pub fn update(&self, id: Id, element: UiElement<Id>) -> TreeInserter<Id>
    {
        TreeInserter{
            elements: &self.created_trees,
            index: 0
        }.update(id, element)
    }

    fn prepare(&mut self)
    {
        let mut created_trees = self.created_trees.borrow_mut();

        let mut resolved = HashMap::new();

        const LIMIT: usize = 1000;
        for i in 0..LIMIT
        {
            TreeElement::resolve_forward(&mut created_trees, 0, None, &mut resolved, None);
            TreeElement::resolve_backward(&mut created_trees, 0, &self.sizer);

            if created_trees[0].resolved(&created_trees)
            {
                break;
            }

            if i == (LIMIT - 1)
            {
                eprintln!("{created_trees:#?}");
                panic!("must be resolved");
            }
        }
    }

    pub fn create_renders(
        &mut self,
        create_info: &mut RenderCreateInfo,
        dt: f32
    )
    {
        self.shared.borrow_mut().consecutive = 0;

        self.prepare();

        let mut created_trees = self.created_trees.borrow_mut();

        self.shared.borrow_mut().elements.iter_mut().for_each(|element| element.closing = true);

        let mut last_match = None;
        created_trees[0].for_each(&created_trees, |id, element, deferred|
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

        created_trees.clear();
        created_trees.push(TreeElement::screen(self.shared.clone()));
    }

    pub fn set_mouse_position(&mut self, position: Vector2<f32>)
    {
        self.shared.borrow_mut().mouse_position = position;
    }

    fn set_screen_size(&mut self, size: Vector2<f32>)
    {
        self.sizer.update_screen_size(size);

        self.shared.borrow_mut().screen_size = size;
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.set_screen_size(Vector2::from(info.partial.size));

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
                if let Some(scissor) = cached.scissor
                {
                    info.set_scissor(scissor);
                }

                info.push_constants(UiOutlinedInfo::new(cached.mix));

                object.draw(info);

                if cached.scissor.is_some()
                {
                    info.reset_scissor();
                }
            }
        });
    }
}

pub fn add_padding<Id: Idable>(x: TreeInserter<Id>, width: UiElementSize<Id>, height: UiElementSize<Id>)
{
    let id = x.consecutive();
    x.update(Id::padding(id), UiElement{
        width,
        height,
        ..Default::default()
    });
}

pub fn add_padding_horizontal<Id: Idable>(x: TreeInserter<Id>, size: UiElementSize<Id>)
{
    add_padding(x, size, 0.0.into())
}

pub fn add_padding_vertical<Id: Idable>(x: TreeInserter<Id>, size: UiElementSize<Id>)
{
    add_padding(x, 0.0.into(), size)
}
