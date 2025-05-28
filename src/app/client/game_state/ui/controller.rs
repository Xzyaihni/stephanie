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
    client::RenderCreateInfo,
    common::{
        render_info::*,
        colors::Lcha,
        EaseOut
    }
};

use super::element::*;


pub const MINIMUM_SCALE: f32 = 0.0005;
pub const MINIMUM_DISTANCE: f32 = 0.0001;

pub trait Idable: Hash + Eq + Clone + Debug
{
    fn screen() -> Self;
    fn padding(id: u32) -> Self;
}

pub trait Inputable
{
    fn position_mapped(&self, check_position: Vector2<f32>) -> Option<Vector2<f32>>;
    fn mouse_position(&self) -> Vector2<f32>;

    fn position_inside(&self, check_position: Vector2<f32>) -> Option<Vector2<f32>>
    {
        let mapped = self.position_mapped(check_position)?;

        let r = 0.0..1.0;
        (r.contains(&mapped.x) && r.contains(&mapped.y)).then_some(mapped)
    }

    fn is_inside(&self, check_position: Vector2<f32>) -> bool
    {
        self.position_inside(check_position).is_some()
    }

    fn mouse_position_inside(&self) -> Option<Vector2<f32>>
    {
        self.position_inside(self.mouse_position())
    }

    fn mouse_position_mapped(&self) -> Option<Vector2<f32>>
    {
        self.position_mapped(self.mouse_position())
    }

    fn is_mouse_inside(&self) -> bool
    {
        self.is_inside(self.mouse_position())
    }
}

#[derive(Debug)]
pub struct TreeInserter<'a, Id>
{
    elements: &'a RefCell<Vec<TreeElement<Id>>>,
    index: usize
}

impl<Id> Copy for TreeInserter<'_, Id> {}

impl<Id> Clone for TreeInserter<'_, Id>
{
    fn clone(&self) -> Self { *self }
}

#[allow(dead_code)]
impl<'a, Id: Idable> TreeInserter<'a, Id>
{
    fn tree_element(&self) -> Ref<'a, TreeElement<Id>>
    {
        Ref::map(self.elements.borrow(), |x| &x[self.index])
    }

    fn tree_element_mut(&self) -> RefMut<'a, TreeElement<Id>>
    {
        RefMut::map(self.elements.borrow_mut(), |x| &mut x[self.index])
    }

    pub fn input_of<'b>(&self, id: &'b Id) -> InputHandler<'b, Id>
    {
        let shared = self.tree_element().shared.clone();
        let mouse_position = shared.borrow().mouse_position;

        InputHandler{mouse_position, shared, id}
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

    pub fn screen_size(&self) -> Vector2<f32>
    {
        self.tree_element().shared.borrow().screen_size
    }

    fn persistent_element<T>(&self, f: impl FnOnce(Option<&Element<Id>>) -> T) -> T
    {
        let element = self.tree_element();
        let id = &element.id;
        let shared = element.shared.borrow();

        f(shared.elements.get(id))
    }

    pub fn try_width(&self) -> Option<f32>
    {
        self.persistent_element(|x| x.map(|x| x.deferred.width.unwrap()))
    }

    pub fn try_height(&self) -> Option<f32>
    {
        self.persistent_element(|x| x.map(|x| x.deferred.height.unwrap()))
    }
}

impl<Id: Idable> Inputable for TreeInserter<'_, Id>
{
    fn position_mapped(&self, check_position: Vector2<f32>) -> Option<Vector2<f32>>
    {
        let shared = Ref::map(self.tree_element(), |x| &x.shared);

        let shared = shared.borrow();
        shared.position_mapped(&self.tree_element().id, check_position)
    }

    fn mouse_position(&self) -> Vector2<f32>
    {
        self.tree_element().shared.borrow().mouse_position
    }
}

fn parental_position_of<Id>(parent_deferred: Option<&UiDeferredInfo>, element: &UiElement<Id>) -> Vector2<f32>
{
    element.animation.position
        .as_ref().map(|x| x.parent_relative).unwrap_or(false)
        .then(|| parent_deferred.and_then(|x| x.position))
        .flatten()
        .unwrap_or_else(Vector2::zeros)
}

#[derive(Debug, Clone)]
struct Fractions
{
    scale: Vector2<f32>,
    scale_inherit: Vector2<f32>,
    position: Vector2<f32>,
    position_inherit: Vector2<f32>
}

impl Default for Fractions
{
    fn default() -> Self
    {
        Self{
            scale: Vector2::repeat(1.0),
            scale_inherit: Vector2::repeat(1.0),
            position: Vector2::zeros(),
            position_inherit: Vector2::zeros()
        }
    }
}

#[derive(Debug)]
struct UiElementCached
{
    fractions: Fractions,
    scale: Vector2<f32>,
    parental_position: Vector2<f32>,
    position: Vector2<f32>,
    mix: Option<MixColorLch>,
    scissor: Option<VulkanoScissor>,
    object: Option<ClientRenderObject>
}

impl UiElementCached
{
    fn from_element<Id>(
        create_info: &mut RenderCreateInfo,
        parent_fraction: &Fractions,
        parent_deferred: Option<&UiDeferredInfo>,
        deferred: &UiDeferredInfo,
        element: &UiElement<Id>
    ) -> Self
    {
        let scissor = Self::calculate_scissor(
            Vector2::from(create_info.object_info.partial.size),
            deferred
        );

        let scale = {
            let scaling = element.animation.scaling
                .as_ref()
                .map(|x| x.start_scaling)
                .unwrap_or(Vector2::repeat(1.0));

            let width = deferred.width.unwrap() * scaling.x;
            let height = deferred.height.unwrap() * scaling.y;

            Vector2::new(width, height)
        };

        let parental_position = parental_position_of(parent_deferred, element);

        let position = {
            let offset = element.animation.position
                .as_ref()
                .and_then(|x| x.offsets)
                .map(|x| x.start)
                .unwrap_or_else(Vector2::zeros);

            deferred.position.unwrap() + offset - parental_position
        };

        let transform = Transform::default();

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
            },
            UiTexture::CustomId(id) =>
            {
                RenderObject{
                    kind: RenderObjectKind::TextureId{id: *id}
                }.into_client(transform, create_info)
            }
        };

        let mut this = Self{
            fractions: Fractions::default(),
            scale,
            position,
            parental_position,
            mix: element.mix,
            scissor,
            object
        };

        this.update_fraction(parent_fraction, deferred);

        this
    }

    fn update_fraction(
        &mut self,
        parent_fraction: &Fractions,
        deferred: &UiDeferredInfo
    )
    {
        self.fractions.scale = parent_fraction.scale_inherit;
        self.fractions.position = parent_fraction.position_inherit;

        {
            let target_scale = Vector2::new(deferred.width.unwrap(), deferred.height.unwrap());

            let fraction = self.scale.component_div(&target_scale);
            let fraction = fraction.zip_map(&target_scale, |a, b| if b == 0.0 { 1.0 } else { a });

            self.fractions.scale_inherit = fraction.component_mul(&parent_fraction.scale_inherit);
        }

        {
            let target_position = deferred.position.unwrap() - self.parental_position;

            let fraction = self.position - target_position;

            self.fractions.position_inherit = fraction + parent_fraction.position_inherit;
        }

        if let Some(object) = self.object.as_mut()
        {
            object.modify_transform(|transform|
            {
                let scale = self.scale.component_mul(&self.fractions.scale);
                transform.scale = Vector3::new(scale.x, scale.y, 1.0);

                let position = self.position + self.fractions.position + self.parental_position;
                transform.position = Vector3::new(position.x, position.y, 0.0);
            })
        }
    }

    fn update_always<Id>(
        &mut self,
        parent_fraction: &Fractions,
        deferred: &UiDeferredInfo,
        element: &mut UiElement<Id>,
        screen_size: Vector2<f32>,
        dt: f32
    )
    {
        if let (Some(mix), Some(target)) = (self.mix.as_mut(), element.mix)
        {
            *mix = if let Some(animation) = &element.animation.mix
            {
                macro_rules! mix_color
                {
                    ($($field:ident),+) =>
                    {
                        Lcha{
                            $($field: mix.color.$field.ease_out(target.color.$field, animation.$field, dt),)+
                        }
                    }
                }

                MixColorLch{
                    color: mix_color!(l, c, h, a),
                    ..target
                }
            } else
            {
                target
            };
        }

        self.scissor = Self::calculate_scissor(screen_size, deferred);

        self.update_fraction(parent_fraction, deferred);
    }

    fn update<Id>(
        &mut self,
        create_info: &mut RenderCreateInfo,
        parent_fraction: &Fractions,
        parent_deferred: Option<&UiDeferredInfo>,
        deferred: &UiDeferredInfo,
        old_element: &mut UiElement<Id>,
        dt: f32
    )
    {
        let target_scale = Vector2::new(deferred.width.unwrap(), deferred.height.unwrap());

        if let Some(scaling) = old_element.animation.scaling.as_mut()
        {
            scaling.start_mode.next_2d(&mut self.scale, target_scale, dt);
        } else
        {
            self.scale = target_scale;
        }

        self.parental_position = parental_position_of(parent_deferred, old_element);

        let target_position = deferred.position.unwrap() - self.parental_position;
        if let Some(connection) = old_element.animation.position.as_mut()
        {
            connection.start_mode.simple_next_2d(
                &mut self.position,
                target_position,
                dt
            );
        } else
        {
            self.position = target_position;
        }

        self.update_always(
            parent_fraction,
            deferred,
            old_element,
            Vector2::from(create_info.object_info.partial.size),
            dt
        );

        if self.object.is_none()
        {
            self.object = Self::from_element(
                create_info,
                parent_fraction,
                parent_deferred,
                deferred,
                old_element
            ).object;
        }
    }

    fn calculate_scissor(
        screen_size: Vector2<f32>,
        deferred: &UiDeferredInfo
    ) -> Option<VulkanoScissor>
    {
        deferred.scissor.map(|mut x|
        {
            let highest = screen_size.max();

            let aspect = screen_size / highest;

            x.offset = (Vector2::from(x.offset) + (aspect / 2.0)).into();

            x.into_global([highest, highest])
        })
    }

    fn update_closing<Id>(
        &mut self,
        parent_fraction: &Fractions,
        deferred: &UiDeferredInfo,
        element: &mut UiElement<Id>,
        close_soon: bool,
        screen_size: Vector2<f32>,
        dt: f32
    ) -> bool
    {
        if let Some(scaling) = element.animation.scaling.as_mut()
        {
            if let Scaling::Ignore = scaling.close_mode
            {
                if close_soon
                {
                    return false;
                }
            }

            let close_scaling = scaling.close_scaling.component_mul(&self.scale);

            scaling.close_mode.next_2d(
                &mut self.scale,
                close_scaling,
                dt
            );
        }

        let offset = element.animation.position.as_ref()
            .and_then(|x| x.offsets)
            .map(|x| x.end)
            .unwrap_or_else(Vector2::zeros);

        let target_position = deferred.position.unwrap() + offset - self.parental_position;
        if let Some(connection) = element.animation.position.as_mut()
        {
            connection.close_mode.simple_next_2d(
                &mut self.position,
                target_position,
                dt
            );
        } else
        {
            self.position = target_position;
        }

        if element.animation.scaling.is_none() && element.animation.position.is_none()
        {
            if close_soon
            {
                return false;
            }
        }

        self.update_always(parent_fraction, deferred, element, screen_size, dt);

        if self.fractions.scale.min() < MINIMUM_SCALE
        {
            return false;
        }

        if let Some(scale) = self.object.as_ref().and_then(|object|
        {
            object.transform().map(|transform| transform.scale.xy().min())
        })
        {
            if scale < MINIMUM_SCALE
            {
                return false;
            }
        }

        if element.animation.scaling.is_none() && element.animation.position.is_some()
        {
            if (target_position - self.position).abs().sum() < MINIMUM_DISTANCE
            {
                return false;
            }
        }

        true
    }

    fn keep_old<Id: Eq>(&self, new: &mut Self, old_element: &UiElement<Id>, new_element: &UiElement<Id>)
    {
        macro_rules! fields_different
        {
            ($($field:ident),+) =>
            {
                false $(|| old_element.$field != new_element.$field)+
            }
        }

        if fields_different!(texture)
        {
            return;
        }

        if new_element.animation.mix.is_some()
        {
            new.mix = self.mix;
        }

        if new_element.animation.position.is_some()
        {
            new.position = self.position;
        }

        if new_element.animation.scaling.is_some()
        {
            new.scale = self.scale;
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
                    let size = Vector2::new(width, height);
                    let offset = position - (size / 2.0);

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
        let resolve_inherit = || -> Option<_>
        {
            let parent_position = parent.position?;
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
        };

        match &element.position
        {
            UiPosition::Absolute{position, align} =>
            {
                let get_offset = |value: f32, size: Option<f32>| -> Option<f32>
                {
                    Some(if value == 0.0 { 0.0 } else { value * size? * 0.5 })
                };

                Some(Vector2::new(
                    position.x + get_offset(align.horizontal.as_value(), self.width.value())?,
                    position.y + get_offset(align.vertical.as_value(), self.height.value())?
                ))
            },
            UiPosition::Offset(id, x) =>
            {
                resolved.get(id).and_then(|element| element.position.map(|pos| pos + *x))
            },
            UiPosition::Next(offset) =>
            {
                Some((if let Some(previous) = previous
                {
                    let parent_position = parent.position?;
                    let previous_position = previous.position?;
                    UiPosition::<Id>::next_position(
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
                    )
                } else
                {
                    resolve_inherit()?
                }) + *offset)
            },
            UiPosition::Inherit => resolve_inherit()
        }
    }

    fn resolve_backward<Id: Idable>(
        &mut self,
        sizer: &TextureSizer,
        element: &UiElement<Id>,
        changes_total: bool,
        children: Vec<ResolvedBackward>
    ) -> ResolvedBackward
    {
        let texture_size = || sizer.size(&element.texture);

        let is_width_parallel = element.children_layout.is_horizontal();

        let resolved = ResolvedBackward{
            width: self.width.resolve_backward(
                || texture_size().x,
                is_width_parallel,
                &element.width,
                children.iter().map(|x| x.width)
            ).map(|value| SizeBackwardInfo{changes_total, value}),
            height: self.height.resolve_backward(
                || texture_size().y,
                !is_width_parallel,
                &element.height,
                children.iter().map(|x| x.height)
            ).map(|value| SizeBackwardInfo{changes_total, value})
        };

        if let UiPosition::Absolute{..} = element.position
        {
            return ResolvedBackward::empty();
        }

        resolved
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
    is_first_child: Option<bool>,
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
            is_first_child: None,
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
            is_first_child: Some(true),
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

            let ignored_total = !this.is_first_child.unwrap() && this.element.position.is_inherit();
            this.deferred.resolve_backward(
                sizer,
                &this.element,
                !ignored_total,
                infos
            )
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

            this.is_first_child = Some(previous.is_none());

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
        mut f: impl FnMut(Option<&Self>, &Self)
    )
    {
        self.for_each_inner(trees, None, &mut f)
    }

    fn for_each_inner(
        &self,
        trees: &Vec<TreeElement<Id>>,
        parent: Option<&Self>,
        f: &mut impl FnMut(Option<&Self>, &Self)
    )
    {
        f(parent, self);
        self.children.iter().for_each(|index|
        {
            trees[*index].for_each_inner(trees, Some(self), f)
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
            | UiTexture::Custom(_)
            | UiTexture::CustomId(_) =>
            {
                (if let UiTexture::CustomId(id) = texture
                {
                    self.assets.lock().texture(*id).lock().size()
                } else
                {
                    self.assets.lock().texture_by_name(texture.name().unwrap()).lock().size()
                }) / self.size.max()
            }
        }
    }
}

#[derive(Debug)]
struct Element<Id>
{
    id: Id,
    parent_fraction: Fractions,
    close_immediately: bool,
    close_soon: bool,
    element: UiElement<Id>,
    cached: UiElementCached,
    deferred: UiDeferredInfo,
    closing: bool
}

#[derive(Debug)]
struct CachedTree<Id>
{
    value: Option<Id>,
    children: Vec<CachedTree<Id>>
}

impl<Id: Idable> Default for CachedTree<Id>
{
    fn default() -> Self
    {
        Self{value: Some(Id::screen()), children: Vec::new()}
    }
}

impl<Id: Idable> CachedTree<Id>
{
    fn for_each(&self, mut f: impl FnMut(Option<&Id>, &Id))
    {
        self.for_each_inner(None, &mut f)
    }

    fn for_each_inner(&self, parent: Option<&Id>, f: &mut impl FnMut(Option<&Id>, &Id))
    {
        if let Some(id) = self.value.as_ref()
        {
            f(parent, id);
        }

        self.children.iter().for_each(|x| x.for_each_inner(self.value.as_ref(), f));
    }

    fn remove(&mut self, id: &Id) -> bool
    {
        if self.value.as_ref() == Some(id)
        {
            self.value = None;
        } else
        {
            self.children.retain_mut(|child| !child.remove(id));
        }

        self.value.is_none() && self.children.is_empty()
    }

    fn push_child(&mut self, parent: &Id, value: &Id) -> bool
    {
        if self.value.as_ref() == Some(value)
        {
            return true;
        }

        if self.value.as_ref() == Some(parent)
        {
            if self.children.iter().all(|child| child.value.as_ref() != Some(value))
            {
                self.children.push(CachedTree{value: Some(value.clone()), children: Vec::new()});
            }

            true
        } else
        {
            self.children.iter_mut().any(|x| x.push_child(parent, value))
        }
    }
}

#[derive(Debug)]
struct SharedInfo<Id>
{
    consecutive: u32,
    mouse_position: Vector2<f32>,
    screen_size: Vector2<f32>,
    elements: HashMap<Id, Element<Id>>,
    tree: CachedTree<Id>
}

impl<Id: Idable> SharedInfo<Id>
{
    pub fn new() -> Self
    {
        Self{
            consecutive: 0,
            mouse_position: Vector2::zeros(),
            screen_size: Vector2::repeat(1.0),
            elements: HashMap::new(),
            tree: Default::default()
        }
    }

    pub fn aspect(&self) -> Vector2<f32>
    {
        self.screen_size / self.screen_size.max()
    }

    pub fn position_mapped(&self, id: &Id, check_position: Vector2<f32>) -> Option<Vector2<f32>>
    {
        self.elements.get(id).map(|element|
        {
            let deferred = &element.deferred;

            let position = deferred.position.unwrap();
            let size = Vector2::new(deferred.width.unwrap(), deferred.height.unwrap());

            ((check_position - position) + (size / 2.0)).component_div(&size)
        })
    }
}

pub struct InputHandler<'a, Id>
{
    mouse_position: Vector2<f32>,
    shared: Rc<RefCell<SharedInfo<Id>>>,
    id: &'a Id
}

impl<Id: Idable> Inputable for InputHandler<'_, Id>
{
    fn position_mapped(&self, check_position: Vector2<f32>) -> Option<Vector2<f32>>
    {
        self.shared.borrow().position_mapped(self.id, check_position)
    }

    fn mouse_position(&self) -> Vector2<f32>
    {
        self.mouse_position
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

    pub fn as_inserter(&self) -> TreeInserter<Id>
    {
        TreeInserter{
            elements: &self.created_trees,
            index: 0
        }
    }

    pub fn update(&self, id: Id, element: UiElement<Id>) -> TreeInserter<Id>
    {
        self.as_inserter().update(id, element)
    }

    pub fn input_of<'a>(&self, id: &'a Id) -> InputHandler<'a, Id>
    {
        InputHandler{mouse_position: self.shared.borrow().mouse_position, shared: self.shared.clone(), id}
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

        self.shared.borrow_mut().elements.values_mut().for_each(|element| element.closing = true);

        let mut replace_tree = CachedTree{
            value: Some(Id::screen()),
            children: Vec::new()
        };

        created_trees[0].for_each(&created_trees, |parent, this|
        {
            let mut shared = self.shared.borrow_mut();

            {
                let [element, parent] = if let Some(parent) = parent.as_ref()
                {
                    shared.elements.get_disjoint_mut([&this.id, &parent.id])
                } else
                {
                    [shared.elements.get_mut(&this.id), None]
                };

                let parent_fraction = parent.as_ref().map(|x| x.cached.fractions.clone()).unwrap_or_default();

                if let Some(element) = element
                {
                    if element.element != this.element
                    {
                        let mut cached = UiElementCached::from_element(
                            create_info,
                            &parent_fraction,
                            parent.as_ref().map(|x| &x.deferred),
                            &this.deferred,
                            &this.element
                        );

                        element.cached.keep_old(&mut cached, &element.element, &this.element);

                        element.cached = cached;

                        element.element = this.element.clone();
                    }

                    element.closing = false;
                    element.deferred = this.deferred.clone();

                    element.cached.update(
                        create_info,
                        &parent_fraction,
                        parent.as_ref().map(|x| &x.deferred),
                        &this.deferred,
                        &mut element.element,
                        dt
                    );
                } else
                {
                    let cached = UiElementCached::from_element(
                        create_info,
                        &parent_fraction,
                        parent.as_ref().map(|x| &x.deferred),
                        &this.deferred,
                        &this.element
                    );

                    let mut element = Element{
                        id: this.id.clone(),
                        parent_fraction: parent_fraction.clone(),
                        close_immediately: false,
                        close_soon: false,
                        element: this.element.clone(),
                        cached,
                        deferred: this.deferred.clone(),
                        closing: false
                    };

                    element.cached.update(
                        create_info,
                        &parent_fraction,
                        parent.as_ref().map(|x| &x.deferred),
                        &this.deferred,
                        &mut element.element,
                        dt
                    );

                    shared.elements.insert(element.id.clone(), element);
                }
            }

            let id = &this.id;

            if let Some(parent) = parent.as_ref()
            {
                replace_tree.push_child(&parent.id, id);
            }

            if id != &Id::screen()
            {
                shared.tree.remove(id);
            }
        });

        {
            let mut shared = self.shared.borrow_mut();
            let shared: &mut SharedInfo<Id> = &mut shared;

            replace_tree.for_each(|parent_id, id|
            {
                if let Some(parent_id) = parent_id
                {
                    shared.tree.push_child(parent_id, id);
                } else
                {
                    shared.tree.push_child(&Id::screen(), id);
                }
            });

            shared.tree.for_each(|parent, id|
            {
                let [element, parent] = if let Some(parent) = parent
                {
                    shared.elements.get_disjoint_mut([id, parent])
                } else
                {
                    [shared.elements.get_mut(id), None]
                };

                let element = element.unwrap();

                if element.closing
                {
                    if let Some(parent) = parent
                    {
                        if !parent.closing
                        {
                            element.close_soon = true;
                        } else
                        {
                            element.parent_fraction = parent.cached.fractions.clone();
                        }
                    } else
                    {
                        element.close_immediately = true;
                    }
                }
            });
        }

        {
            let mut shared = self.shared.borrow_mut();
            let shared: &mut SharedInfo<Id> = &mut shared;

            let screen_size = shared.screen_size;

            let closer = |element: &mut Element<Id>|
            {
                if element.closing
                {
                    if element.close_immediately
                    {
                        return false;
                    }

                    element.cached.update_closing(
                        &element.parent_fraction,
                        &element.deferred,
                        &mut element.element,
                        element.close_soon,
                        screen_size,
                        dt
                    )
                } else
                {
                    true
                }
            };

            shared.elements.retain(|id, element|
            {
                let keep = closer(element);

                if !keep
                {
                    shared.tree.remove(id);
                }

                keep
            });
        }

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

    #[allow(dead_code)]
    pub fn screen_size(&self) -> Vector2<f32>
    {
        self.shared.borrow().screen_size
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        self.set_screen_size(Vector2::from(info.partial.size));

        let mut shared = self.shared.borrow_mut();
        let shared: &mut SharedInfo<Id> = &mut shared;

        shared.tree.for_each(|_, id|
        {
            if let Some(object) = shared.elements.get_mut(id).unwrap().cached.object.as_mut()
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
        let shared = self.shared.borrow();
        shared.tree.for_each(|_, id|
        {
            let element = shared.elements.get(id).unwrap();
            let cached = &element.cached;
            if let Some(object) = cached.object.as_ref()
            {
                if let Some(scissor) = cached.scissor
                {
                    info.set_scissor(scissor);
                }

                info.push_constants(UiOutlinedInfo::new(cached.mix.map(|x| x.into())));

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

#[allow(dead_code)]
pub fn add_padding_vertical<Id: Idable>(x: TreeInserter<Id>, size: UiElementSize<Id>)
{
    add_padding(x, 0.0.into(), size)
}
