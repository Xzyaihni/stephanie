/*use std::{
    rc::{Weak, Rc},
    cell::{Ref, RefMut, RefCell},
    ops::{Range, ControlFlow, Index}
};

use nalgebra::Vector2;

use crate::{
    client::game_state::MousePosition,
    common::animator::Animatable
};


#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComplexId(usize);

#[derive(Debug, Clone, PartialEq)]
pub enum ElementId
{
    Primitive(ElementPrimitiveId),
    Complex(ComplexId)
}

impl<'a> TryFrom<&'a ElementId> for &'a ElementPrimitiveId
{
    type Error = ();

    fn try_from(id: &ElementId) -> Result<&ElementPrimitiveId, Self::Error>
    {
        match id
        {
            ElementId::Primitive(x) => Ok(x),
            _ => Err(())
        }
    }
}

impl TryFrom<&ElementId> for ComplexId
{
    type Error = ();

    fn try_from(id: &ElementId) -> Result<Self, Self::Error>
    {
        match id
        {
            ElementId::Complex(x) => Ok(*x),
            _ => Err(())
        }
    }
}

enum PrimitiveIdWrapper<'a>
{
    Ref(&'a ElementPrimitiveId),
    Cell(Ref<'a, ElementPrimitiveId>)
}

impl<'a> From<&'a ElementPrimitiveId> for PrimitiveIdWrapper<'a>
{
    fn from(x: &'a ElementPrimitiveId) -> Self
    {
        Self::Ref(x)
    }
}

impl<'a> From<Ref<'a, ElementPrimitiveId>> for PrimitiveIdWrapper<'a>
{
    fn from(x: Ref<'a, ElementPrimitiveId>) -> Self
    {
        Self::Cell(x)
    }
}

impl<'a> PrimitiveIdWrapper<'a>
{
    fn as_ref(&self) -> &ElementPrimitiveId
    {
        match self
        {
            Self::Ref(x) => x,
            Self::Cell(x) => &x
        }
    }

    fn as_owned(self) -> ElementPrimitiveId
    {
        match self
        {
            Self::Ref(x) => x.clone(),
            Self::Cell(x) => x.clone()
        }
    }
}

impl ElementId
{
    fn primitive_id<'a>(&'a self, ui: &'a Ui) -> PrimitiveIdWrapper<'a>
    {
        match self
        {
            Self::Primitive(x) => PrimitiveIdWrapper::Ref(x),
            Self::Complex(x) => PrimitiveIdWrapper::Cell(ui.complex_to_primitive(*x))
        }
    }
}

trait TopLevelAdder
{
    fn add(&self, ui: &mut Ui, element: UiElementPrimitive) -> ElementPrimitiveId;
}

struct TopLevelAdderNormal;

impl TopLevelAdder for TopLevelAdderNormal
{
    fn add(&self, ui: &mut Ui, element: UiElementPrimitive) -> ElementPrimitiveId
    {
        ui.push(element).primitive_id(ui).as_owned()
    }
}

struct TopLevelAdderChild<'a>
{
    parent: &'a ElementId,
}

impl<'a> TopLevelAdder for TopLevelAdderChild<'a>
{
    fn add(&self, ui: &mut Ui, element: UiElementPrimitive) -> ElementPrimitiveId
    {
        ui.push_child(self.parent, element).primitive_id(ui).as_owned()
    }
}

struct ElementAdder<'a, TopLevel>
{
    ui: &'a mut Ui,
    adder: TopLevel
}

impl<'a, T: TopLevelAdder> ElementAdder<'a, T>
{
    fn top_level(&mut self, element: UiElementPrimitive) -> ElementPrimitiveId
    {
        self.adder.add(self.ui, element)
    }

    fn child(
        &self,
        parent: &ElementPrimitiveId,
        element: impl Into<UiElementPrimitiveWithFlags>
    ) -> ElementPrimitiveId
    {
        element.into().new_child_inner(self.ui, parent)
    }
}

#[derive(Debug, Clone)]
pub struct ScrollElement
{
    dragging: bool,
    scroll: f32,
    body: Rc<RefCell<UiPrimitive>>,
    body_id: ElementPrimitiveId,
    background: Rc<RefCell<UiPrimitive>>,
    bar_rail: Rc<RefCell<UiPrimitive>>,
    bar: Rc<RefCell<UiPrimitive>>,
    textures: ScrollTextures
}

impl PartialEq for ScrollElement
{
    fn eq(&self, other: &Self) -> bool
    {
        self.body_id == other.body_id
    }
}

/*impl ScrollElement
{
    fn new_with<T: TopLevelAdder>(
        info: ScrollElementInfo,
        adder: &mut ElementAdder<T>
    ) -> Self
    {
        let body_id = adder.top_level(UiElementPrimitive{
            kind: UiElementType::Panel,
            pos: info.pos,
            size: info.size,
            texture: None
        });

        let background_id = adder.child(&body_id, UiElementPrimitive{
            kind: UiElementType::Panel,
            pos: Vector2::repeat(0.0),
            size: Vector2::repeat(1.0).into(),
            texture: Some(info.textures.background.normal)
        });

        let rail_size = 1.0 - info.bar_size;

        let bar_rail_id = adder.child(&body_id, UiElementPrimitive{
            kind: UiElementType::Button,
            pos: Vector2::new(0.0, (1.0 - rail_size) * 0.5),
            size: Vector2::new(1.0, rail_size).into(),
            texture: None
        });

        let bar_id = adder.child(&bar_rail_id, UiElementPrimitive{
            kind: UiElementType::Panel,
            pos: Vector2::repeat(0.0),
            size: Vector2::new(1.0, info.bar_size / rail_size).into(),
            texture: Some(info.textures.scrollbar.normal)
        });

        let body = adder.ui.get_primitive(&body_id);
        let background = adder.ui.get_primitive(&background_id);
        let bar_rail = adder.ui.get_primitive(&bar_rail_id);
        let bar = adder.ui.get_primitive(&bar_id);

        Self{
            dragging: false,
            scroll: 0.0,
            body,
            background,
            body_id,
            bar_rail,
            bar,
            textures: info.textures
        }
    }

    pub fn scroll(&self) -> f32
    {
        self.scroll
    }

    pub fn set_scroll(&mut self, new_scroll: f32)
    {
        self.scroll = new_scroll;

        self.update_bar();
    }

    fn update_bar(&mut self)
    {
        self.bar.set(UiAnimatableId::PositionCenteredY, self.scroll);
    }

    fn mouse_move(&mut self, pos: Vector2<f32>)
    {
        if self.dragging
        {
            let pos = self.bar_rail.borrow().element().local_position(pos);

            self.set_scroll(pos.y.clamp(0.0, 1.0));
        }
        
        self.update_textures(pos);
    }

    fn mouse_state(&mut self, down: bool, pos: Vector2<f32>)
    {
        if down
        {
            let inside = self.bar_rail.borrow().element().intersects(pos);
            let edge = self.body.borrow().element().intersects(pos);

            if inside || edge
            {
                self.set_pressed(pos, true);
            }

            self.mouse_move(pos);
        } else
        {
            self.set_pressed(pos, false);
        }
    }

    fn set_pressed(&mut self, pos: Vector2<f32>, state: bool)
    {
        self.dragging = state;
        
        self.update_textures(pos);
    }

    fn update_textures(&mut self, pos: Vector2<f32>)
    {
        self.set_button_texture(pos, &self.background, &self.textures.background);
        self.set_button_texture(pos, &self.bar, &self.textures.scrollbar);
    }

    fn set_button_texture(
        &self,
        pos: Vector2<f32>,
        element: &Rc<RefCell<UiPrimitive>>,
        textures: &ButtonTextures
    )
    {
        textures.set_button_texture(pos, element, self.dragging)
    }
}*/

pub struct Children<'a>(&'a [Item]);

impl<'a> Index<usize> for Children<'a>
{
    type Output = ElementId;

    fn index(&self, index: usize) -> &Self::Output
    {
        &self.0[index].id
    }
}

impl<'a> Children<'a>
{
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item=&'a ElementId>
    {
        self.0.iter().map(|x| &x.id)
    }

    pub fn frames_iter(&self) -> impl Iterator<Item=&'a ElementId>
    {
        self.0.iter().map(|x| &x.frame_id)
    }
}

#[derive(Debug, Clone)]
struct Item
{
    id: ElementId,
    frame_id: ElementId,
    frame: Rc<RefCell<UiPrimitive>>,
    value: Rc<RefCell<UiPrimitive>>
}

#[derive(Debug, Clone)]
pub struct ListElement
{
    items: Vec<Item>,
    background: Rc<RefCell<UiPrimitive>>,
    body_id: ElementPrimitiveId,
    scroll: Rc<RefCell<UiComplex>>,
    item_height: f32,
    last_scroll: f32,
    draw_range: Range<usize>,
    mouse_pos: MousePosition,
    selected_index: Option<usize>,
    textures: ButtonTextures
}

impl PartialEq for ListElement
{
    fn eq(&self, other: &Self) -> bool
    {
        self.body_id == other.body_id
    }
}

impl ListElement
{
    fn new_with<T: TopLevelAdder>(
        info: ListElementInfo,
        adder: &mut ElementAdder<T>
    ) -> Self
    {
        let body_id = ElementId::Primitive(adder.top_level(UiElementPrimitive{
            kind: UiElementType::Panel,
            pos: info.pos,
            size: info.size,
            texture: None
        }));

        let scrollbar_width = 0.1;
        let bar_size = 0.1;

        let scroll = UiElementComplex::Scroll(ScrollElementInfo{
            pos: Vector2::new(1.0 - scrollbar_width, 0.0),
            size: Vector2::new(scrollbar_width, 1.0).into(),
            bar_size,
            textures: info.scroll_textures
        });

        let scroll = scroll.new_child(&mut adder.ui, &body_id);
        let scroll = adder.ui.get_complex(scroll).clone();

        // insanity
        let body_id = if let ElementId::Primitive(x) = body_id
        {
            x
        } else
        {
            unreachable!()
        };

        let background_id = adder.child(&body_id, UiElementPrimitive{
            kind: UiElementType::Panel,
            pos: Vector2::new(0.0, 0.0),
            size: Vector2::new(1.0 - scrollbar_width, 1.0).into(),
            texture: Some(info.background)
        }.default_flags().no_draw());

        let items = info.items.into_iter().map(|element|
        {
            let container = adder.child(&background_id, UiElementPrimitive{
                kind: UiElementType::Button,
                pos: Vector2::new(0.0, 0.0),
                size: Vector2::new(1.0, info.item_height).into(),
                texture: Some(info.textures.normal)
            });

            let id = adder.child(&container, element);

            Item{
                value: adder.ui.get_primitive(&id).clone(),
                frame: adder.ui.get_primitive(&container).clone(),
                id: ElementId::Primitive(id),
                frame_id: ElementId::Primitive(container)
            }
        }).collect();

        let background = adder.ui.get_primitive(&background_id).clone();

        let mut this = Self{
            items,
            background,
            body_id,
            item_height: info.item_height,
            last_scroll: 0.0,
            scroll,
            draw_range: 0..0,
            mouse_pos: Vector2::repeat(0.0),
            selected_index: None,
            textures: info.textures
        };

        this.update_scroll(this.get_scroll());

        this
    }

    pub fn children(&self) -> Children
    {
        Children(&self.items)
    }

    fn get_scroll(&self) -> f32
    {
        let scroll = self.scroll.borrow();

        match &*scroll
        {
            UiComplex::Scroll(x) => x.scroll(),
            _ => unreachable!()
        }
    }

    fn fits(&self) -> f32
    {
        1.0 / self.item_height
    }

    fn start_index(&self) -> f32
    {
        let fits = self.fits();

        let height = 1.0 - self.last_scroll;

        let last_item = self.items.len().saturating_sub(fits as usize);
        height * last_item as f32
    }

    fn drawing_range(&self) -> Range<usize>
    {
        let start_index = self.start_index();

        let offset = start_index.fract();
        let start_index = start_index as usize;

        let len = (self.fits() + offset).ceil() as usize;
        let end_index = (start_index + len).min(self.items.len());

        start_index..end_index
    }

    fn draw_custom(&self, ui: &Ui)
    {
        let background = &self.background.borrow().element;
        ui.draw_element(background);

        let parent_size = ui.pixels_size_inner(&background).map(|x| x as usize);

        let clip_texture = ui.temporary_texture(parent_size);
        let mut clip_texture = clip_texture.borrow_mut();

        let size = parent_size.map(|x| x as f32);

        let assets = ui.ui.assets.borrow();
        self.items[self.draw_range.clone()].iter().for_each(|item|
        {
            let value = &item.value.borrow().element;

            let frame = &item.frame.borrow().element;

            let pos = frame.inner.pos * size;

            let frame_size = frame.inner.size.to_size(1.0) * size;
            let value_size = value.inner.size.to_size(frame.global_size.aspect());

            let size = frame_size * value_size;

            if let Some(texture_id) = frame.inner.texture
            {
                let rect = Self::create_rect(parent_size.y as i32, pos, frame_size);
                let texture = assets.texture(texture_id);

                ui.draw_texture_to_texture(&mut clip_texture, &texture, rect);
            }

            if let Some(texture_id) = value.inner.texture
            {
                let rect = Self::create_rect(parent_size.y as i32, pos, size);
                let texture = assets.texture(texture_id);

                ui.draw_texture_to_texture(&mut clip_texture, &texture, rect);
            }
        });

        ui.draw_texture(&clip_texture, background);
    }

    /*fn create_rect(parent_height: i32, pos: Vector2<f32>, size: Vector2<f32>) -> Rect
    {
        let pos = pos.map(|x| x as i32);
        let size = size.map(|x| x as u32 + 1);

        Rect::new(
            pos.x,
            parent_height - pos.y - size.y as i32,
            size.x,
            size.y
        )
    }*/

    fn update_scroll(&mut self, scroll: f32)
    {
        self.last_scroll = scroll;
        self.draw_range = self.drawing_range();
        
        let offset = self.start_index().fract();

        self.items[self.draw_range.clone()].iter_mut().enumerate().for_each(|(index, item)|
        {
            let y = (index as f32 - offset) * self.item_height;

            item.frame.set(UiAnimatableId::PositionY, 1.0 - y - self.item_height);
        });
    }

    // i dont know if i need the &Ui anymore, im too tired of constantly removing it
    fn mouse_move(&mut self, _ui: &Ui, pos: Vector2<f32>)
    {
        let scroll = self.get_scroll();

        if self.last_scroll != scroll
        {
            self.update_scroll(scroll);
        }

        self.mouse_pos = pos;

        self.update_textures();
    }

    fn update_textures(&mut self)
    {
        self.items[self.draw_range.clone()].iter_mut().enumerate().for_each(|(index, item)|
        {
            let mut frame = item.frame.borrow_mut();

            let new_texture = 
            if self.selected_index == Some(index)
            {
                self.textures.pressed
            } else
            {
                if frame.element.intersects(self.mouse_pos)
                {
                    self.textures.hover
                } else
                {
                    self.textures.normal
                }
            };

            frame.set_texture(new_texture);
        });
    }

    fn mouse_state(&mut self, ui: &Ui, down: bool, pos: Vector2<f32>)
    {
        if down
        {
            self.items[self.draw_range.clone()].iter().enumerate().any(|(index, item)|
            {
                let frame = item.frame.borrow();
                if frame.element.intersects(pos)
                {
                    self.selected_index = Some(index);

                    true
                } else
                {
                    false
                }
            });
        }

        self.mouse_move(ui, pos);
    }

    #[allow(dead_code)]
    pub fn selected(&self) -> Option<usize>
    {
        self.selected_index
    }

    #[allow(dead_code)]
    pub fn select_index(&mut self, index: usize)
    {
        self.selected_index = Some(index);
        self.update_textures();
    }

    #[allow(dead_code)]
    pub fn deselect(&mut self)
    {
        self.selected_index = None;
        self.update_textures();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiComplex
{
    Scroll(ScrollElement),
    List(ListElement)
}

impl UiComplex
{
    fn new_with<T: TopLevelAdder>(
        value: UiElementComplex,
        mut adder: ElementAdder<T>
    ) -> Self
    {
        match value
        {
            UiElementComplex::Scroll(x) => Self::Scroll(ScrollElement::new_with(x, &mut adder)),
            UiElementComplex::List(x) => Self::List(ListElement::new_with(x, &mut adder))
        }
    }

    fn draw_custom(&self, ui: &Ui)
    {
        match self
        {
            Self::List(x) => x.draw_custom(ui),
            _ => ()
        }
    }

    fn mouse_move(&mut self, ui: &Ui, pos: Vector2<f32>)
    {
        match self
        {
            Self::Scroll(x) => x.mouse_move(pos),
            Self::List(x) => x.mouse_move(ui, pos)
        }
    }

    fn mouse_state(&mut self, ui: &Ui, down: bool, pos: Vector2<f32>)
    {
        match self
        {
            Self::Scroll(x) => x.mouse_state(down, pos),
            Self::List(x) => x.mouse_state(ui, down, pos)
        }
    }
}

// i could just store the children in a vec but this is much cooler
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementPrimitiveId
{
    id: usize,
    child: Option<Box<Self>>
}

impl ElementPrimitiveId
{
    pub fn new(id: usize) -> Self
    {
        Self{id, child: None}
    }

    pub fn push(&self, child_id: usize) -> Self
    {
        let mut element = self.clone();

        element.set_tail(child_id);

        element
    }

    pub fn set_tail(&mut self, child_id: usize)
    {
        if let Some(child_element) = self.child.as_mut()
        {
            child_element.set_tail(child_id)
        } else
        {
            self.child = Some(Box::new(Self::new(child_id)));
        }
    }
}

pub struct UiEvent
{
    pub element_id: ElementPrimitiveId
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum UiElementType
{
    Panel,
    Button
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum StretchMode
{
    Min,
    Max,
    LockX,
    LockY
}

#[derive(Debug, Clone, Copy)]
pub struct KeepAspect
{
    aspect: f32,
    mode: StretchMode,
    size: Vector2<f32>
}

impl KeepAspect
{
    pub fn new(mode: StretchMode, size: Vector2<f32>) -> Self
    {
        Self{aspect: 1.0, mode, size}
    }
}

#[derive(Debug, Clone, Copy)]
pub enum UiSize
{
    KeepAspect(KeepAspect),
    Normal(Vector2<f32>)
}

impl From<KeepAspect> for UiSize
{
    fn from(value: KeepAspect) -> Self
    {
        Self::KeepAspect(value)
    }
}

impl From<Vector2<f32>> for UiSize
{
    fn from(value: Vector2<f32>) -> Self
    {
        Self::Normal(value)
    }
}

impl UiSize
{
    pub fn to_size(self, local_aspect: f32) -> Vector2<f32>
    {
        match self
        {
            Self::Normal(x) => x,
            Self::KeepAspect(KeepAspect{aspect, mode, size: p}) =>
            {
                let aspect = aspect * local_aspect;

                match mode
                {
                    StretchMode::Min | StretchMode::Max =>
                    {
                        let change_x = match mode
                        {
                            StretchMode::Min => aspect > 1.0,
                            StretchMode::Max => aspect < 1.0,
                            _ => unreachable!()
                        };

                        if change_x
                        {
                            Vector2::new(p.x / aspect, p.y)
                        } else
                        {
                            Vector2::new(p.x, p.y * aspect)
                        }
                    },
                    StretchMode::LockX =>
                    {
                        if aspect > 1.0
                        {
                            Vector2::new(p.x, p.y * aspect)
                        } else
                        {
                            Vector2::new(p.x, p.y * aspect)
                        }
                    },
                    StretchMode::LockY =>
                    {
                        if aspect > 1.0
                        {
                            Vector2::new(p.x / aspect, p.y)
                        } else
                        {
                            Vector2::new(p.x / aspect, p.y)
                        }
                    }
                }
            }
        }
    }

    fn set_aspect(&mut self, new_aspect: f32)
    {
        match self
        {
            Self::KeepAspect(KeepAspect{aspect, ..}) => *aspect = new_aspect,
            _ => ()
        }
    }

    pub fn set_x(&mut self, x: f32)
    {
        self.inner_mut().x = x;
    }

    pub fn set_y(&mut self, y: f32)
    {
        self.inner_mut().y = y;
    }

    #[allow(dead_code)]
    pub fn map(mut self, f: impl FnOnce(Vector2<f32>) -> Vector2<f32>) -> Self
    {
        let p = self.inner_mut();

        *p = f(*p);

        self
    }

    fn inner_mut(&mut self) -> &mut Vector2<f32>
    {
        match self
        {
            Self::Normal(x) => x,
            Self::KeepAspect(KeepAspect{size, ..}) => size
        }
    }
}

#[derive(Debug, Clone)]
struct PrimitiveFlags
{
    no_draw: bool
}

impl Default for PrimitiveFlags
{
    fn default() -> Self
    {
        Self{no_draw: false}
    }
}

#[derive(Debug, Clone)]
pub struct UiElementPrimitiveWithFlags
{
    inner: UiElementPrimitive,
    flags: PrimitiveFlags
}

impl From<UiElementPrimitive> for UiElementPrimitiveWithFlags
{
    fn from(x: UiElementPrimitive) -> Self
    {
        x.default_flags()
    }
}

impl UiElementPrimitiveWithFlags
{
    pub fn no_draw(mut self) -> Self
    {
        self.flags.no_draw = true;

        self
    }
}

pub enum UiElement
{
    Complex(UiElementComplex),
    Primitive(UiElementPrimitiveWithFlags)
}

impl From<UiElementPrimitiveWithFlags> for UiElement
{
    fn from(x: UiElementPrimitiveWithFlags) -> Self
    {
        Self::Primitive(x)
    }
}

impl From<UiElementPrimitive> for UiElement
{
    fn from(x: UiElementPrimitive) -> Self
    {
        UiElementPrimitiveWithFlags::from(x).into()
    }
}

impl From<UiElementComplex> for UiElement
{
    fn from(x: UiElementComplex) -> Self
    {
        Self::Complex(x)
    }
}

impl UiElement
{
    fn new(self, ui: &mut Ui) -> ElementId
    {
        match self
        {
            Self::Complex(x) => ElementId::Complex(x.new(ui)),
            Self::Primitive(x) => ElementId::Primitive(x.new(ui))
        }
    }

    fn new_child(self, ui: &mut Ui, parent: &ElementId) -> ElementId
    {
        match self
        {
            Self::Complex(x) => ElementId::Complex(x.new_child(ui, parent)),
            Self::Primitive(x) => ElementId::Primitive(x.new_child(ui, parent))
        }
    }
}

#[derive(Debug, Clone)]
pub struct ButtonTextures
{
    pub normal: TextureId,
    pub hover: TextureId,
    pub pressed: TextureId
}

impl ButtonTextures
{
    pub fn repeat(t: TextureId) -> Self
    {
        Self{
            normal: t,
            hover: t,
            pressed: t
        }
    }

    fn set_button_texture(
        &self,
        pos: Vector2<f32>,
        element: &Rc<RefCell<UiPrimitive>>,
        is_pressed: bool
    )
    {
        let mut element = element.borrow_mut();

        let new_texture = if is_pressed
        {
            self.pressed
        } else
        {
            if element.element.intersects(pos)
            {
                self.hover
            } else
            {
                self.normal
            }
        };

        element.set_texture(new_texture);
    }
}

#[derive(Debug, Clone)]
pub struct ScrollTextures
{
    pub background: ButtonTextures,
    pub scrollbar: ButtonTextures
}

#[derive(Debug, Clone)]
pub struct ScrollElementInfo
{
    pub pos: Vector2<f32>,
    pub size: UiSize,
    pub bar_size: f32,
    pub textures: ScrollTextures
}

#[derive(Debug, Clone)]
pub struct ListElementInfo
{
    pub items: Vec<UiElementPrimitive>,
    pub pos: Vector2<f32>,
    pub size: UiSize,
    pub item_height: f32,
    pub background: TextureId,
    pub textures: ButtonTextures,
    pub scroll_textures: ScrollTextures
}

#[derive(Debug, Clone)]
pub enum UiElementComplex
{
    Scroll(ScrollElementInfo),
    List(ListElementInfo)
}

impl UiElementComplex
{
    fn new(self, ui: &mut Ui) -> ComplexId
    {
        let element = UiComplex::new_with(self, ElementAdder{ui, adder: TopLevelAdderNormal});

        Self::into_id(ui, element)
    }

    fn new_child(self, ui: &mut Ui, parent: &ElementId) -> ComplexId
    {
        let element = UiComplex::new_with(
            self,
            ElementAdder{ui, adder: TopLevelAdderChild{parent}}
        );

        Self::into_id(ui, element)
    }

    fn into_id(ui: &mut Ui, element: UiComplex) -> ComplexId
    {
        ui.push_complex(Rc::new(RefCell::new(element)))
    }
}

#[derive(Debug, Clone)]
pub struct UiElementPrimitive
{
    pub kind: UiElementType,
    pub pos: Vector2<f32>,
    pub size: UiSize,
    pub texture: Option<TextureId>
}

impl UiElementPrimitive
{
    pub fn default_flags(self) -> UiElementPrimitiveWithFlags
    {
        UiElementPrimitiveWithFlags{
            inner: self,
            flags: Default::default()
        }
    }
}

impl UiElementPrimitiveWithFlags
{
    fn new(self, ui: &mut Ui) -> ElementPrimitiveId
    {
        let id = ui.ui.elements.len();

        let element = UiPrimitive::new_parent(self);
        UiPrimitive::full_update(element.clone(), ui.aspect());

        ui.ui.elements.push(element);

        ElementPrimitiveId::new(id)
    }

    fn new_child(self, ui: &Ui, parent: &ElementId) -> ElementPrimitiveId
    {
        self.new_child_inner(ui, parent.primitive_id(ui))
    }

    fn new_child_inner<'a>(
        self,
        ui: &Ui,
        parent: impl Into<PrimitiveIdWrapper<'a>>
    ) -> ElementPrimitiveId
    {
        let parent = parent.into();
        let parent = parent.as_ref();

        let id = UiPrimitive::push(&ui.get_primitive(parent), self, ui.aspect());

        parent.push(id)
    }
}

#[derive(Debug, Clone)]
pub struct UiElementGlobal
{
    inner: UiElementPrimitive,
    flags: PrimitiveFlags,
    global_size: Vector2<f32>,
    global_pos: Vector2<f32>
}

impl UiElementGlobal
{
    pub fn intersects(&self, pos: Vector2<f32>) -> bool
    {
        (self.global_pos.x..=(self.global_pos.x + self.global_size.x)).contains(&pos.x)
            && (self.global_pos.y..=(self.global_pos.y + self.global_size.y)).contains(&pos.y)
    }

    pub fn local_position(&self, pos: Vector2<f32>) -> Vector2<f32>
    {
        (pos - self.global_pos) / self.global_size
    }

    pub fn inside_position(&self, pos: Vector2<f32>) -> Option<Vector2<f32>>
    {
        self.intersects(pos).then(|| self.local_position(pos))
    }

    pub fn inside_position_saturating(&self, pos: Vector2<f32>) -> Vector2<f32>
    {
        self.local_position(pos).map(|x| x.clamp(0.0, 1.0))
    }

    fn set_aspect(&mut self, aspect: f32)
    {
        self.inner.size.set_aspect(aspect);
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum UiAnimatableId
{
    ScaleX,
    ScaleY,
    PositionX,
    PositionY,
    PositionCenteredX,
    PositionCenteredY
}

#[derive(Debug)]
pub struct UiPrimitive
{
    parent: Option<(usize, Weak<RefCell<Self>>)>,
    element: UiElementGlobal,
    children: Vec<Rc<RefCell<Self>>>
}

impl UiPrimitive
{
    #[allow(dead_code)]
    pub fn texture(&mut self) -> &mut Option<TextureId>
    {
        &mut self.element.inner.texture
    }

    pub fn set_texture(&mut self, new_texture: TextureId)
    {
        self.element.inner.texture = Some(new_texture);
    }

    pub fn element(&self) -> &UiElementGlobal
    {
        &self.element
    }

    fn new_parent(element: UiElementPrimitiveWithFlags) -> Rc<RefCell<Self>>
    {
        Self::new_inner(None, element)
    }

    fn new_child(
        parent: Weak<RefCell<Self>>,
        id: usize,
        element: UiElementPrimitiveWithFlags
    ) -> Rc<RefCell<Self>>
    {
        Self::new_inner(Some((id, parent)), element)
    }

    fn new_inner(
        parent: Option<(usize, Weak<RefCell<Self>>)>,
        element: UiElementPrimitiveWithFlags
    ) -> Rc<RefCell<Self>>
    {
        let zero = Vector2::<f32>::zero();

        Rc::new(RefCell::new(Self{
            parent,
            element: UiElementGlobal{
                global_size: zero,
                global_pos: zero,
                inner: element.inner,
                flags: element.flags
            },
            children: Vec::new()
        }))
    }

    fn push(
        this: &Rc<RefCell<Self>>,
        element: UiElementPrimitiveWithFlags,
        aspect: f32
    ) -> usize
    {
        let parent = this.clone();

        let id;
        let child;

        {
            let mut this = this.borrow_mut();

            id = this.children.len();

            child = Self::new_child(Rc::downgrade(&parent), id, element);

            this.children.push(child.clone());
        }

        Self::full_update(child, aspect);

        id
    }

    fn update_child(&mut self, id: usize)
    {
        let mut child = self.children[id].borrow_mut();
        let this = &mut self.element;
        
        {
            let child = &mut child.element;

            let aspect = this.global_size.aspect();

            child.global_pos = this.global_pos + child.inner.pos * this.global_size;
            child.global_size = child.inner.size.to_size(aspect) * this.global_size;
        }

        child.update_children();
    }

    fn update_children(&mut self)
    {
        for i in 0..self.children.len()
        {
            self.update_child(i);
        }
    }

    fn full_update(this: Rc<RefCell<Self>>, aspect: f32)
    {
        this.borrow_mut().update_aspect(aspect);
        Self::update(this);
    }

    fn update_aspect(&mut self, aspect: f32)
    {
        self.element.set_aspect(aspect);

        for child in self.children.iter()
        {
            child.borrow_mut().update_aspect(aspect);
        }
    }

    fn update(this: Rc<RefCell<Self>>)
    {
        let pair = this.borrow().parent.clone();
        if let Some((id, parent)) = pair
        {
            parent.upgrade().unwrap().borrow_mut().update_child(id);
        } else
        {
            let mut this = this.borrow_mut();

            this.element.global_pos = this.element.inner.pos;
            this.element.global_size = this.element.inner.size.to_size(1.0);

            this.update_children();
        }
    }

    pub fn size(&self) -> Vector2<f32>
    {
        let aspect = if let Some((_id, parent)) = self.parent.clone()
        {
            parent.upgrade().unwrap().borrow().element.global_size.aspect()
        } else
        {
            1.0
        };

        self.element.inner.size.to_size(aspect)
    }

    fn get(&self, id: &ElementPrimitiveId) -> Rc<RefCell<Self>>
    {
        let this = &self.children[id.id];

        if let Some(child_id) = id.child.as_ref()
        {
            this.borrow().get(child_id)
        } else
        {
            this.clone()
        }
    }

    fn try_for_each_element<T, Q, F>(
        &self,
        query: &Q,
        id: ElementPrimitiveId,
        f: &mut F
    ) -> ControlFlow<T>
    where
        Q: Fn(&Self) -> bool,
        F: FnMut(&ElementPrimitiveId, &UiElementGlobal) -> ControlFlow<T>
    {
        match f(&id, &self.element)
        {
            ControlFlow::Continue(_) => (),
            x => return x
        }

        self.children.iter().enumerate().try_for_each(|(index, child)|
        {
            let child = child.borrow();

            if query(&child)
            {
                let id = id.push(index);

                child.try_for_each_element(query, id, f)
            } else
            {
                ControlFlow::Continue(())
            }
        })
    }
}

impl Animatable<UiAnimatableId> for Rc<RefCell<UiPrimitive>>
{
    fn set(&mut self, id: UiAnimatableId, value: f32)
    {
        {
            let mut this = self.borrow_mut();
            let size = this.size();

            let element = &mut this.element.inner;
            match id
            {
                UiAnimatableId::ScaleX =>
                {
                    element.size.set_x(value);
                },
                UiAnimatableId::ScaleY =>
                {
                    element.size.set_y(value);
                },
                UiAnimatableId::PositionX =>
                {
                    element.pos.x = value;
                },
                UiAnimatableId::PositionY =>
                {
                    element.pos.y = value;
                },
                UiAnimatableId::PositionCenteredX =>
                {
                    element.pos.x = value - (size.x * 0.5);
                },
                UiAnimatableId::PositionCenteredY =>
                {
                    element.pos.y = value - (size.y * 0.5);
                }
            }
        }

        UiPrimitive::update(self.clone());
    }
}

/*struct TemporaryTexture<'a>
{
    size: Vector2<usize>,
    id: TextureId,
    inner: Ref<'a, Assets>
}

impl<'a> TemporaryTexture<'a>
{
    pub fn borrow_mut(&self) -> RefMut<'_, Texture<'static>>
    {
        let image = Image::repeat(self.size.x, self.size.y, Color{r: 0, g: 0, b: 0, a: 0});

        self.inner.update_texture(self.id, &image)
    }
}

struct UiGeneral
{
    window: Rc<RefCell<WindowWrapper>>,
    assets: Rc<RefCell<Assets>>,
    elements: Vec<Rc<RefCell<UiPrimitive>>>,
    temporary_texture: TextureId
}

pub struct Ui
{
    complex: Vec<Rc<RefCell<UiComplex>>>,
    // i thought i needed a separate reference but im stupid ahhhhhhh, wutever
    ui: UiGeneral
}

#[allow(dead_code)]
impl Ui
{
    pub fn new(window: Rc<RefCell<WindowWrapper>>, assets: Rc<RefCell<Assets>>) -> Self
    {
        let empty_image = Image::repeat(1, 1, Color{r: 0, g: 0, b: 0, a: 0});
        let temporary_texture = assets.borrow_mut().add_texture_access(
            TextureAccess::Target,
            &empty_image
        );

        let ui = UiGeneral{
            window,
            assets,
            elements: Vec::new(),
            temporary_texture
        };

        Self{complex: Vec::new(), ui}
    }

    pub fn push(&mut self, element: impl Into<UiElement>) -> ElementId
    {
        element.into().new(self)
    }

    pub fn push_child(
        &mut self,
        parent_id: &ElementId,
        element: impl Into<UiElement>
    ) -> ElementId
    {
        element.into().new_child(self, parent_id)
    }

    pub fn get(&self, id: &ElementId) -> Rc<RefCell<UiPrimitive>>
    {
        let id = id.primitive_id(self);

        self.get_primitive(id.as_ref())
    }

    pub fn get_primitive<'a>(
        &self,
        id: impl TryInto<&'a ElementPrimitiveId>
    ) -> Rc<RefCell<UiPrimitive>>
    {
        let id = id.try_into().unwrap_or_else(|_| panic!("id isnt primitive"));
        let this = &self.ui.elements[id.id];

        if let Some(child_id) = id.child.as_ref()
        {
            this.borrow().get(child_id)
        } else
        {
            this.clone()
        }
    }

    pub fn get_complex(&self, id: impl TryInto<ComplexId>) -> &Rc<RefCell<UiComplex>>
    {
        let id = id.try_into().unwrap_or_else(|_| panic!("id isnt primitive"));
        &self.complex[id.0]
    }

    fn complex_to_primitive(&self, id: ComplexId) -> Ref<ElementPrimitiveId>
    {
        let complex = self.get_complex(id);

        Ref::map(complex.borrow(), |complex|
        {
            match complex
            {
                UiComplex::Scroll(x) => &x.body_id,
                UiComplex::List(x) => &x.body_id
            }
        })
    }

    fn push_complex(&mut self, element: Rc<RefCell<UiComplex>>) -> ComplexId
    {
        let id = self.complex.len();

        self.complex.push(element);

        ComplexId(id)
    }

    pub fn pixels_size(&self, id: &ElementId) -> Vector2<u32>
    {
        let ele = self.get(id);
        let ele = ele.borrow();

        self.pixels_size_inner(&ele.element)
    }

    fn pixels_size_inner(&self, element: &UiElementGlobal) -> Vector2<u32>
    {
        let window_size = self.window_size().map(|x| x as f32);

        (element.global_size * window_size)
            .map(|x| x.round() as u32)
    }

    fn aspect(&self) -> f32
    {
        self.window_size().map(|x| x as f32).aspect()
    }

    fn window_size(&self) -> Vector2<u32>
    {
        self.ui.window.borrow().window_size()
    }

    pub fn resized(&mut self)
    {
        for element in self.ui.elements.iter()
        {
            UiPrimitive::full_update(element.clone(), self.aspect());
        }
    }

    fn temporary_texture(&self, size: Vector2<usize>) -> TemporaryTexture
    {
        TemporaryTexture{size, id: self.ui.temporary_texture, inner: self.ui.assets.borrow()}
    }

    fn draw_element(&self, element: &UiElementGlobal)
    {
        if let Some(texture_id) = element.inner.texture
        {
            let assets = self.ui.assets.borrow();

            self.draw_texture(&assets.texture(texture_id), element);
        }
    }

    fn draw_texture(&self, texture: &Texture, element: &UiElementGlobal)
    {
        let rect = self.element_rect(element);
        
        self.ui.window.borrow_mut().canvas
            .copy(texture, None, rect)
            .unwrap();
    }

    fn draw_to_texture(&self, target_texture: &mut Texture, element: &UiElementGlobal)
    {
        if let Some(texture_id) = element.inner.texture
        {
            let assets = self.ui.assets.borrow();

            let rect = self.element_rect(element);
            let texture = assets.texture(texture_id);

            self.draw_texture_to_texture(target_texture, &texture, rect)
        }
    }

    fn draw_texture_to_texture(
        &self,
        target_texture: &mut Texture,
        draw_texture: &Texture,
        rect: Rect
    )
    {
        self.ui.window.borrow_mut().canvas
            .with_texture_canvas(target_texture, |canvas|
            {
                canvas.copy(draw_texture, None, rect).unwrap()
            })
            .unwrap();
    }

    fn draw_with(&self, f: impl FnOnce(&Texture, Rect), element: &UiElementGlobal)
    {
        if let Some(texture_id) = element.inner.texture
        {
            let assets = self.ui.assets.borrow();

            let rect = self.element_rect(element);
            let texture = assets.texture(texture_id);

            f(&texture, rect)
        }
    }

    fn element_rect(&self, element: &UiElementGlobal) -> Rect
    {
        let window_size = self.window_size().map(|x| x as f32);

        let scaled_pos = {
            let mut pos = element.global_pos;

            pos.y = 1.0 - pos.y - element.global_size.y;

            pos * window_size
        }.map(|x| x.round() as i32);

        let x = scaled_pos.x;
        let y = scaled_pos.y;

        let Vector2::new(width, height) = self.pixels_size_inner(element);

        Rect::new(x, y, width, height)
    }

    pub fn draw(&self)
    {
        self.for_each_element(&|element|
        {
            !element.element.flags.no_draw
        }, |_id, element|
        {
            self.draw_element(element);
        });

        self.complex.iter().for_each(|complex|
        {
            complex.borrow().draw_custom(self);
        });
    }

    pub fn mouse_move(&self, pos: Vector2<f32>)
    {
        self.complex.iter().for_each(|complex|
        {
            complex.borrow_mut().mouse_move(self, pos);
        });
    }

    pub fn mouse_down(&self, pos: Vector2<f32>) -> Option<UiEvent>
    {
        self.mouse_state(true, pos)
    }

    pub fn mouse_up(&self, pos: Vector2<f32>) -> Option<UiEvent>
    {
        self.mouse_state(false, pos)
    }

    pub fn mouse_state(&self, down: bool, pos: Vector2<f32>) -> Option<UiEvent>
    {
        self.complex.iter().for_each(|complex|
        {
            complex.borrow_mut().mouse_state(self, down, pos);
        });

        let flow = self.try_for_each_element(&|_| true, |id, element|
        {
            match element.inner.kind
            {
                UiElementType::Button =>
                {
                    if down && element.intersects(pos)
                    {
                        return ControlFlow::Break(UiEvent{element_id: id.clone()});
                    }
                },
                UiElementType::Panel => ()
            }

            ControlFlow::Continue(())
        });

        match flow
        {
            ControlFlow::Break(x) => Some(x),
            ControlFlow::Continue(_) => None
        }
    }

    fn try_for_each_element<T, Q, F>(&self, query: &Q, mut f: F) -> ControlFlow<T>
    where
        Q: Fn(&UiPrimitive) -> bool,
        F: FnMut(&ElementPrimitiveId, &UiElementGlobal) -> ControlFlow<T>
    {
        self.ui.elements.iter().enumerate().try_for_each(|(index, element)|
        {
            let element = element.borrow();

            if query(&element)
            {
                let id = ElementPrimitiveId::new(index);

                element.try_for_each_element(query, id, &mut f)
            } else
            {
                ControlFlow::Continue(())
            }
        })
    }

    fn for_each_element<Q, F>(&self, query: &Q, mut f: F)
    where
        Q: Fn(&UiPrimitive) -> bool,
        F: FnMut(&ElementPrimitiveId, &UiElementGlobal)
    {
        self.try_for_each_element(query, |id, element|
        {
            f(id, element);

            ControlFlow::<()>::Continue(())
        });
    }
}*/*/
