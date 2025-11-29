use std::{
    f32,
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
    DefaultTexture,
    TextureId,
    ShaderId,
    game_object::*
};

use crate::common::{
    render_info::*,
    colors::Lcha,
    EaseOut
};

pub use super::super::element::*;


pub const MINIMUM_SCALE: f32 = 0.0005;
pub const MINIMUM_DISTANCE: f32 = 0.0001;
pub const MINIMUM_COLOR_DISTANCE: f32 = 0.01;

pub trait Idable: Hash + Eq + Clone + Debug
{
    fn screen() -> Self;
    fn padding(id: u32) -> Self;
}

pub trait Inputable
{
    fn position_mapped(&self, check_position: Vector2<f32>) -> Option<Vector2<f32>>;
    fn mouse_position(&self) -> Vector2<f32>;

    fn try_width(&self) -> Option<f32>;
    fn try_height(&self) -> Option<f32>;
    fn try_position(&self) -> Option<Vector2<f32>>;

    fn exists(&self) -> bool;

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

pub struct UiShaders
{
    pub ui: ShaderId,
    pub ui_fill: ShaderId
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
        debug_assert!(!self.elements.borrow().iter().any(|x| x.id == id), "{id:?} was defined multiple times");

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

    pub fn element(&self) -> RefMut<'_, UiElement<Id>>
    {
        RefMut::map(self.tree_element_mut(), |x| x.element())
    }

    pub fn screen_size(&self) -> Vector2<f32>
    {
        self.tree_element().shared.borrow().screen_size
    }

    pub fn pixels_size(&self, pixels: Vector2<f32>) -> Vector2<f32>
    {
        pixels / self.screen_size().max()
    }

    fn persistent_element<T>(&self, f: impl FnOnce(Option<&Element<Id>>) -> T) -> T
    {
        let element = self.tree_element();
        let id = &element.id;
        let shared = element.shared.borrow();

        f(shared.elements.get(id))
    }

    pub fn is_mix_near(&self) -> Option<bool>
    {
        self.persistent_element(|x|
        {
            x.and_then(|x| -> Option<bool>
            {
                Some(is_mix_near(x.cached.mix?.color, x.element.mix?.color))
            })
        })
    }

    pub fn try_width_animated(&self) -> Option<f32>
    {
        self.persistent_element(|x| x.map(|x| x.cached.current_scale().x))
    }

    pub fn try_height_animated(&self) -> Option<f32>
    {
        self.persistent_element(|x| x.map(|x| x.cached.current_scale().y))
    }

    pub fn try_position_animated(&self) -> Option<Vector2<f32>>
    {
        self.persistent_element(|x| x.map(|x| x.cached.current_position()))
    }

    pub fn try_position(&self) -> Option<Vector2<f32>>
    {
        self.persistent_element(|x| x.map(|x| x.deferred.position.unwrap()))
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

    fn try_width(&self) -> Option<f32>
    {
        Self::try_width(self)
    }

    fn try_height(&self) -> Option<f32>
    {
        Self::try_height(self)
    }

    fn try_position(&self) -> Option<Vector2<f32>>
    {
        Self::try_position(self)
    }

    fn exists(&self) -> bool
    {
        self.persistent_element(|x| x.is_some())
    }
}

fn parental_position_of<Id>(parent_deferred: Option<&UiDeferredInfo<Id>>, element: &UiElement<Id>) -> Vector2<f32>
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
    alpha: f32,
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
            alpha: 1.0,
            scale: Vector2::repeat(1.0),
            scale_inherit: Vector2::repeat(1.0),
            position: Vector2::zeros(),
            position_inherit: Vector2::zeros()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct UiScissor
{
    pub size: Vector2<f32>,
    pub position: Vector2<f32>
}

#[derive(Debug)]
struct UiElementCached
{
    fractions: Fractions,
    scale: Vector2<f32>,
    parental_position: Vector2<f32>,
    position: Vector2<f32>,
    inherit_animation: bool,
    mix: Option<MixColorLch>,
    last_scissor: Option<UiScissor>,
    scissor: Option<VulkanoScissor>,
    object: Option<ClientRenderObject>
}

impl UiElementCached
{
    fn from_element<Id>(
        create_info: &mut UpdateBuffersInfo,
        parent_fraction: &Fractions,
        parent_deferred: Option<&UiDeferredInfo<Id>>,
        scissor: Option<UiScissor>,
        deferred: &UiDeferredInfo<Id>,
        element: &UiElement<Id>
    ) -> Self
    {
        let last_scissor = scissor;
        let scissor = Self::calculate_scissor(Vector2::from(create_info.partial.size), scissor);

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

        let object = Self::create_object(create_info, &element.texture);

        let mix = element.mix.map(|mix|
        {
            let color = element.animation.mix.as_ref().and_then(|x| x.start_mix).unwrap_or(mix.color);

            MixColorLch{color, ..mix}
        });

        let mut this = Self{
            fractions: Fractions::default(),
            scale,
            position,
            parental_position,
            inherit_animation: element.inherit_animation,
            mix,
            last_scissor,
            scissor,
            object
        };

        this.update_fraction(parent_fraction, deferred);

        this
    }

    fn create_object(
        create_info: &mut UpdateBuffersInfo,
        texture: &UiTexture
    ) -> Option<ClientRenderObject>
    {
        let kind = match texture
        {
            UiTexture::None => None,
            UiTexture::Text(info) =>
            {
                Some(RenderObjectKind::Text(info.clone()))
            },
            UiTexture::Solid =>
            {
                let id = create_info.partial.assets.lock().default_texture(DefaultTexture::Solid);

                Some(RenderObjectKind::TextureId{id})
            },
            UiTexture::Custom(name) =>
            {
                Some(RenderObjectKind::Texture{name: name.clone()})
            },
            UiTexture::CustomId(id) =>
            {
                Some(RenderObjectKind::TextureId{id: *id})
            },
            UiTexture::Sliced(sliced) =>
            {
                let normal_scale = texture_screen_size(&create_info.partial.assets.lock(), create_info.partial.size.into(), sliced.id);
                Some(RenderObjectKind::TextureSliced{texture: *sliced, normal_scale})
            }
        };

        kind.and_then(|kind|
        {
            RenderObject{
                kind
            }.into_client(Transform::default(), create_info)
        })
    }

    fn update_fraction<Id>(
        &mut self,
        parent_fraction: &Fractions,
        deferred: &UiDeferredInfo<Id>
    )
    {
        self.fractions.scale = if self.inherit_animation { parent_fraction.scale_inherit } else { Vector2::repeat(1.0) };
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

        {
            let scale = self.current_scale();
            let position = self.current_position();

            if let Some(object) = self.object.as_mut()
            {
                object.modify_transform(|transform|
                {
                    transform.scale = Vector3::new(scale.x, scale.y, 1.0);
                    transform.position = Vector3::new(position.x, position.y, 0.0);
                })
            }
        }
    }

    fn current_position(&self) -> Vector2<f32>
    {
        self.position + self.fractions.position + self.parental_position
    }

    fn current_scale(&self) -> Vector2<f32>
    {
        self.scale.component_mul(&self.fractions.scale)
    }

    fn scissor(&self) -> UiScissor
    {
        UiScissor{
            position: self.current_position(),
            size: self.current_scale()
        }
    }

    fn update_mix<Id>(
        &mut self,
        element: &UiElement<Id>,
        target_mix: Option<MixColorLch>,
        alpha_inherit: f32,
        dt: f32
    )
    {
        let target = if let Some(mut x) = target_mix
        {
            x.color.a *= alpha_inherit;

            x
        } else
        {
            self.mix = None;

            return;
        };

        if let Some(mix) = self.mix.as_mut()
        {
            *mix = if let Some(animation) = &element.animation.mix
            {
                macro_rules! mix_color
                {
                    ($($field:ident),+) =>
                    {
                        Lcha{
                            $($field: mix.color.$field.ease_out(target.color.$field, animation.decay.$field, dt),)+
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
    }

    fn update_always<Id>(
        &mut self,
        parent_fraction: &Fractions,
        scissor: Option<UiScissor>,
        deferred: &UiDeferredInfo<Id>,
        screen_size: Vector2<f32>
    )
    {
        self.scissor = Self::calculate_scissor(screen_size, scissor);

        self.update_fraction(parent_fraction, deferred);
    }

    #[allow(clippy::too_many_arguments)]
    fn update<Id>(
        &mut self,
        create_info: &mut UpdateBuffersInfo,
        parent_fraction: &Fractions,
        parent_deferred: Option<&UiDeferredInfo<Id>>,
        scissor: Option<UiScissor>,
        deferred: &UiDeferredInfo<Id>,
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

        self.update_mix(old_element, old_element.mix, parent_fraction.alpha, dt);

        self.last_scissor = scissor;
        self.update_always(
            parent_fraction,
            scissor,
            deferred,
            Vector2::from(create_info.partial.size)
        );

        if self.object.is_none()
        {
            self.object = Self::from_element(
                create_info,
                parent_fraction,
                parent_deferred,
                scissor,
                deferred,
                old_element
            ).object;
        }
    }

    fn calculate_scissor(
        screen_size: Vector2<f32>,
        scissor: Option<UiScissor>
    ) -> Option<VulkanoScissor>
    {
        scissor.map(|UiScissor{size, position}|
        {
            let offset = position - (size / 2.0);

            let mut scissor = Scissor{offset: offset.into(), extent: size.into()};
            let highest = screen_size.max();

            let aspect = screen_size / highest;

            scissor.offset = (Vector2::from(scissor.offset) + (aspect / 2.0)).into();

            scissor.into_global([highest, highest])
        })
    }

    fn update_closing<Id>(
        &mut self,
        parent_fraction: &Fractions,
        deferred: &UiDeferredInfo<Id>,
        element: &mut UiElement<Id>,
        close_soon: bool,
        screen_size: Vector2<f32>,
        dt: f32
    ) -> bool
    {
        let is_scaling_close = element.animation.scaling.as_ref().map(|x| x.close_mode != Scaling::Ignore)
            .unwrap_or(false);

        let is_position_close = element.animation.position.as_ref().map(|x| x.close_mode != Connection::Ignore)
            .unwrap_or(false);

        let is_mix_close = element.animation.mix.as_ref().map(|x| x.close_mix.is_some())
            .unwrap_or(false);

        if let Some(scaling) = element.animation.scaling.as_mut()
        {
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

        if !is_scaling_close && !is_position_close && !is_mix_close
        {
            if close_soon
            {
                return false;
            }
        }

        let target_mix = if let Some(x) = element.animation.mix.as_ref().and_then(|x| x.close_mix)
        {
            element.mix.map(|mix| MixColorLch{color: x, ..mix})
        } else
        {
            element.mix
        };

        {
            let this_alpha = if element.animation.mix.as_ref().map(|x| x.close_mix.is_some()).unwrap_or(false)
            {
                self.mix.map(|x| x.color.a).unwrap_or(1.0)
            } else
            {
                1.0
            };

            let inherit_alpha = if self.inherit_animation { parent_fraction.alpha } else { 1.0 };

            self.fractions.alpha = inherit_alpha * this_alpha;
        }

        self.update_mix(element, target_mix, parent_fraction.alpha, dt);

        self.update_always(
            parent_fraction,
            self.last_scissor,
            deferred,
            screen_size
        );

        if self.fractions.scale.min() < MINIMUM_SCALE
        {
            return false;
        }

        if self.scale.min() < MINIMUM_SCALE
        {
            return false;
        }

        if !is_scaling_close && is_position_close
        {
            if (target_position - self.position).abs().sum() < MINIMUM_DISTANCE
            {
                return false;
            }
        }

        if !is_scaling_close && is_mix_close
        {
            if is_mix_near(self.mix.expect("must be mix close").color, target_mix.expect("must be mix close").color)
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
pub struct UiDeferredInfo<Id>
{
    scissor: Option<Id>,
    position: Option<Vector2<f32>>,
    width: ResolvedSize,
    height: ResolvedSize
}

impl<Id> Default for UiDeferredInfo<Id>
{
    fn default() -> Self
    {
        Self{
            scissor: None,
            position: None,
            width: ResolvedSize::default(),
            height: ResolvedSize::default()
        }
    }
}

impl<Id: Idable> UiDeferredInfo<Id>
{
    fn screen(aspect: Vector2<f32>) -> Self
    {
        let one = |v| ResolvedSize{minimum_size: None, size: Some(v)};

        Self{
            scissor: None,
            position: Some(Vector2::zeros()),
            width: one(aspect.x),
            height: one(aspect.y)
        }
    }

    fn resolve_forward(
        &mut self,
        resolved: &HashMap<Id, Self>,
        screen_size: &Vector2<f32>,
        element: &UiElement<Id>,
        previous: Option<&Self>,
        parent_info: &TreeElement<Id>
    )
    {
        let parent_element = &parent_info.element;
        let parent = &parent_info.deferred;

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

        if parent_element.scissor
        {
            debug_assert!(!element.scissor, "nested scissors not supported");

            self.scissor = Some(parent_info.id.clone());
        } else if let Some(scissor) = parent.scissor.clone()
        {
            debug_assert!(!element.scissor, "nested scissors not supported");

            self.scissor = Some(scissor);
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

    fn resolve_position(
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

    fn resolve_backward(
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
    }
}

pub struct TreeElement<Id>
{
    id: Id,
    element: UiElement<Id>,
    deferred: UiDeferredInfo<Id>,
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
        resolved: &mut HashMap<Id, UiDeferredInfo<Id>>,
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

                infos.push(Self::resolve_backward(trees, resolved, x, sizer));
            }

            infos
        };

        let resolved = {
            let this = &mut trees[index];

            let previously_resolved = this.deferred.resolved();

            let ignored_total = !this.is_first_child.unwrap() && this.element.position.is_inherit();
            let resolved_info = this.deferred.resolve_backward(
                sizer,
                &this.element,
                !ignored_total,
                infos
            );

            if !previously_resolved && this.deferred.resolved()
            {
                resolved.insert(this.id.clone(), this.deferred.clone());
            }

            resolved_info
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
        resolved: &mut HashMap<Id, UiDeferredInfo<Id>>,
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
                parent
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

pub fn texture_screen_size(
    assets: &Assets,
    size: Vector2<f32>,
    texture: TextureId
) -> Vector2<f32>
{
    let texture = assets.texture(texture);

    let this_size = texture.lock().size();
    this_size / size.max()
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
            UiTexture::Text(info) =>
            {
                TextObject::calculate_bounds(
                    info,
                    self.fonts.default_font(),
                    &self.size
                ).component_mul(&(self.size / self.size.max()))
            },
            UiTexture::Solid
            | UiTexture::Custom(_)
            | UiTexture::CustomId(_)
            | UiTexture::Sliced(_) =>
            {
                let assets = self.assets.lock();

                let texture_id = match texture
                {
                    UiTexture::Sliced(sliced) => sliced.id,
                    UiTexture::CustomId(id) => *id,
                    UiTexture::Solid =>
                    {
                        assets.default_texture(DefaultTexture::Solid)
                    },
                    UiTexture::Custom(name) =>
                    {
                        assets.texture_id(name)
                    },
                    _ => unreachable!()
                };

                texture_screen_size(&assets, self.size, texture_id)
            }
        }
    }
}

#[derive(Debug)]
struct Element<Id>
{
    id: Id,
    parent_fraction: Fractions,
    close_soon: bool,
    element: UiElement<Id>,
    cached: UiElementCached,
    deferred: UiDeferredInfo<Id>,
    closing: bool
}

impl<Id> Element<Id>
{
    fn scissor(&self) -> UiScissor
    {
        self.cached.scissor()
    }
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

    fn try_width(&self) -> Option<f32>
    {
        self.persistent_element(|x| x.map(|x| x.deferred.width.unwrap()))
    }

    fn try_height(&self) -> Option<f32>
    {
        self.persistent_element(|x| x.map(|x| x.deferred.height.unwrap()))
    }

    fn try_position(&self) -> Option<Vector2<f32>>
    {
        self.persistent_element(|x| x.map(|x| x.deferred.position.unwrap()))
    }

    fn exists(&self) -> bool
    {
        self.persistent_element(|x| x.is_some())
    }
}

impl<Id: Idable> InputHandler<'_, Id>
{
    fn persistent_element<T>(&self, f: impl FnOnce(Option<&Element<Id>>) -> T) -> T
    {
        let shared = self.shared.borrow();

        f(shared.elements.get(self.id))
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
        let mut this = Self{
            sizer: TextureSizer::new(info),
            created_trees: RefCell::new(vec![TreeElement::screen(shared.clone())]),
            shared
        };

        this.set_screen_size(info.size.into());

        this
    }

    pub fn as_inserter(&self) -> TreeInserter<'_, Id>
    {
        TreeInserter{
            elements: &self.created_trees,
            index: 0
        }
    }

    pub fn update(&self, id: Id, element: UiElement<Id>) -> TreeInserter<'_, Id>
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
            TreeElement::resolve_backward(&mut created_trees, &mut resolved, 0, &self.sizer);

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
        create_info: &mut UpdateBuffersInfo,
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

            let scissor = if let Some(override_id) = this.element.scissor_override.as_ref()
            {
                shared.elements.get(override_id).map(|x| x.scissor())
            } else
            {
                this.deferred.scissor.as_ref().and_then(|id|
                {
                    shared.elements.get(id).map(|scissor_element|
                    {
                        scissor_element.scissor()
                    })
                })
            };

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
                            scissor,
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
                        scissor,
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
                        scissor,
                        &this.deferred,
                        &this.element
                    );

                    let mut element = Element{
                        id: this.id.clone(),
                        parent_fraction: parent_fraction.clone(),
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
                        scissor,
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
                        if parent.closing
                        {
                            element.parent_fraction = parent.cached.fractions.clone();
                        } else
                        {
                            element.close_soon = true;
                        }
                    } else
                    {
                        element.close_soon = true;
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

    pub fn mouse_position(&self) -> Vector2<f32>
    {
        self.shared.borrow().mouse_position
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

    #[allow(dead_code)]
    pub fn texture_size(&self, texture: &UiTexture) -> Vector2<f32>
    {
        self.sizer.size(texture)
    }

    pub fn update_buffers(
        &mut self,
        info: &mut UpdateBuffersInfo
    )
    {
        let old_screen_size = self.shared.borrow().screen_size;
        let new_screen_size = Vector2::from(info.partial.size);

        let screen_size_changed = old_screen_size != new_screen_size;

        if screen_size_changed
        {
            self.set_screen_size(new_screen_size);
        }

        let mut shared = self.shared.borrow_mut();
        let shared: &mut SharedInfo<Id> = &mut shared;

        shared.tree.for_each(|_, id|
        {
            let element = shared.elements.get_mut(id).unwrap();

            if screen_size_changed && matches!(element.element.texture, UiTexture::Sliced(_))
            {
                element.cached.object = UiElementCached::create_object(info, &element.element.texture);
            }

            if let Some(object) = element.cached.object.as_mut()
            {
                object.update_buffers(info)
            }
        });
    }

    pub fn draw(
        &self,
        info: &mut DrawInfo,
        shaders: &UiShaders
    )
    {
        let switch_to_shader = |info: &mut DrawInfo, shader|
        {
            if info.current_pipeline_id() != Some(shader)
            {
                info.bind_pipeline(shader);
            }
        };

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

                if let Some(fill) = element.element.fill
                {
                    switch_to_shader(info, shaders.ui_fill);
                    info.push_constants(fill.into_info(cached.mix.map(|x| x.into())));
                } else
                {
                    switch_to_shader(info, shaders.ui);
                    info.push_constants(UiOutlinedInfo::new(cached.mix.map(|x| x.into())));
                }

                object.draw(info);

                if cached.scissor.is_some()
                {
                    info.reset_scissor();
                }
            }
        });
    }
}

fn is_mix_near(current: Lcha, target: Lcha) -> bool
{
    let distance = (target.l - current.l).abs()
        + (target.c - current.c).abs()
        + (target.h - current.h).abs() * (100.0 / (f32::consts::PI * 2.0))
        + (target.a - current.a).abs() * 100.0;

    distance < MINIMUM_COLOR_DISTANCE
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
