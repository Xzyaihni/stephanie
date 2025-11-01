use std::{
    fs,
    path::PathBuf,
    rc::Rc,
    collections::HashMap
};

use nalgebra::{vector, Matrix4};

use yanyaengine::{
    game_object::*,
    FontsContainer,
    Control,
    SolidObject,
    camera::Camera
};

use crate::{
    app::ProgramShaders,
    client::{
        self,
        SlicedTexture,
        game_state::{
            UiControls,
            ControlsController,
            ui::{BACKGROUND_COLOR, ACCENT_COLOR, controller::*}
        }
    },
    common::{
        some_or_value,
        render_info::*,
        lazy_transform::SpringScalingInfo
    }
};


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MainMenuId
{
    Screen,
    Padding(u32),
    Menu,
    Main(MainPartId),
    Options(OptionsPartId),
    WorldSelect(WorldSelectPartId)
}

impl Idable for MainMenuId
{
    fn screen() -> Self
    {
        Self::Screen
    }

    fn padding(id: u32) -> Self
    {
        Self::Padding(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MainPartId
{
    Title(AlignPartId),
    Buttons(AlignPartId),
    Start(ButtonPartId),
    Options(ButtonPartId),
    Quit(ButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AlignPartId
{
    Outer,
    Inner
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum OptionsPartId
{
    Controls(ButtonPartId),
    Back(ButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WorldSelectPartId
{
    Panel,
    PanelInner,
    Message,
    Textbox,
    Buttons,
    Confirm(ButtonPartId),
    Back(ButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ButtonPartId
{
    Panel,
    Body,
    Text,
    BarOuter,
    Bar,
    BarText
}

pub enum MenuAction
{
    None,
    Quit,
    Start(MenuClientInfo)
}

#[derive(Clone, Copy)]
enum MenuState
{
    Main,
    Options,
    WorldSelect
}

const BUTTON_SIZE: f32 = 0.05;
const TEXTBOX_SIZE: f32 = BUTTON_SIZE;

#[derive(Clone)]
pub struct MenuClientInfo
{
    pub address: Option<String>,
    pub name: String,
    pub host: bool,
    pub debug: bool
}

struct WorldInfo
{
    name: String,
    path: PathBuf
}

#[derive(Clone)]
struct ButtonInfo
{
    name: String,
    width: UiElementSize<MainMenuId>,
    height: UiElementSize<MainMenuId>,
    body_width: UiElementSize<MainMenuId>,
    override_font_size: Option<u32>,
    padding_left: UiElementSize<MainMenuId>,
    padding_right: UiElementSize<MainMenuId>,
    align_left: bool,
    invert_colors: bool
}

pub struct MainMenu
{
    shaders: ProgramShaders,
    sliced_textures: Rc<HashMap<String, SlicedTexture>>,
    fonts: Rc<FontsContainer>,
    screen_object: SolidObject,
    controller: Controller<MainMenuId>,
    controls: ControlsController<MainMenuId>,
    state: MenuState,
    ui_camera: Camera,
    worlds: Vec<WorldInfo>,
    info: MenuClientInfo
}

impl MainMenu
{
    pub fn new(
        partial_info: &ObjectCreatePartialInfo,
        shaders: ProgramShaders,
        sliced_textures: Rc<HashMap<String, SlicedTexture>>
    ) -> Self
    {
        let controller = Controller::new(partial_info);

        let info = MenuClientInfo{
            address: None,
            name: "stephanie".to_owned(),
            host: true,
            debug: false
        };

        let ui_camera = Camera::new(partial_info.aspect(), -1.0..1.0);

        let worlds_path = PathBuf::from("worlds");
        let mut worlds = if worlds_path.exists()
        {
            fs::read_dir(worlds_path).and_then(|iter| -> Result<Vec<_>, _>
            {
                iter.map(|x|
                {
                    x.map(|x|
                    {
                        WorldInfo{
                            name: x.file_name().to_string_lossy().into_owned(),
                            path: x.path()
                        }
                    })
                }).collect()
            }).unwrap_or_else(|err|
            {
                eprintln!("error reading worlds: {err}");

                Vec::new()
            })
        } else
        {
            Vec::new()
        };

        worlds.sort_by(|a, b| a.name.cmp(&b.name));

        Self{
            shaders,
            sliced_textures,
            fonts: partial_info.builder_wrapper.fonts().clone(),
            screen_object: client::create_screen_object(partial_info),
            controller,
            controls: ControlsController::new(),
            state: MenuState::Main,
            ui_camera,
            worlds,
            info
        }
    }

    fn update_main_button(
        &self,
        controls: &mut UiControls<MainMenuId>,
        parent: TreeInserter<MainMenuId>,
        id: impl Fn(ButtonPartId) -> MainMenuId,
        name: &str
    ) -> bool
    {
        self.update_button(controls, parent, id, ButtonInfo{
            name: name.to_owned(),
            width: UiSize::Rest(1.0).into(),
            height: UiElementSize{
                minimum_size: Some(UiMinimumSize::Pixels(85.0)),
                size: UiSize::Absolute(BUTTON_SIZE)
            },
            body_width: UiSize::FitChildren.into(),
            override_font_size: None,
            padding_left: 0.01.into(),
            padding_right: 0.005.into(),
            align_left: true,
            invert_colors: false
        })
    }

    fn update_button(
        &self,
        controls: &mut UiControls<MainMenuId>,
        parent: TreeInserter<MainMenuId>,
        id: impl Fn(ButtonPartId) -> MainMenuId,
        info: ButtonInfo
    ) -> bool
    {
        let (primary_color, secondary_color) = if info.invert_colors
        {
            (BACKGROUND_COLOR, ACCENT_COLOR)
        } else
        {
            (ACCENT_COLOR, BACKGROUND_COLOR)
        };

        let panel_id = id(ButtonPartId::Panel);
        let panel = parent.update(panel_id, UiElement{
            width: info.width,
            height: info.height,
            children_layout: if info.align_left { UiLayout::Horizontal } else { UiLayout::Vertical },
            ..Default::default()
        });

        let body = panel.update(id(ButtonPartId::Body), UiElement{
            width: info.body_width,
            height: UiSize::CopyElement(UiDirection::Vertical, 1.0, panel_id).into(),
            position: UiPosition::Inherit,
            children_layout: UiLayout::Horizontal,
            ..Default::default()
        });

        let inside_button = body.is_mouse_inside();

        let width = some_or_value!(body.try_width(), false);
        let height = some_or_value!(body.try_height(), false);

        let outer = panel.update(id(ButtonPartId::BarOuter), UiElement{
            width: width.into(),
            height: UiSize::CopyElement(UiDirection::Vertical, 1.0, panel_id).into(),
            position: UiPosition::Inherit,
            scissor: true,
            ..Default::default()
        });

        let font_size = info.override_font_size.unwrap_or_else(||
        {
            (self.controller.screen_size().max() * height * 0.6) as u32
        });

        let bar_id = id(ButtonPartId::Bar);

        if inside_button
        {
            let outside_offset = vector![-width, 0.0];

            let animation = if info.align_left
            {
                Animation{
                    position: Some(PositionAnimation{
                        offsets: Some(PositionOffsets{
                            start: outside_offset,
                            end: outside_offset
                        }),
                        start_mode: Connection::EaseOut{decay: 20.0, limit: None},
                        close_mode: Connection::EaseOut{decay: 35.0, limit: None},
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            } else
            {
                let scaling = vector![0.0, 1.0];

                Animation{
                    scaling: Some(ScalingAnimation{
                        start_scaling: scaling,
                        start_mode: Scaling::EaseOut{decay: 20.0},
                        close_scaling: scaling,
                        close_mode: Scaling::EaseOut{decay: 35.0},
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            };

            outer.update(bar_id, UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(secondary_color)),
                width: width.into(),
                height: UiSize::Rest(1.0).into(),
                position: UiPosition::Inherit,
                animation,
                ..Default::default()
            });
        }

        if self.controller.input_of(&bar_id).exists()
        {
            if let Some(position) = self.controller.input_of(&id(ButtonPartId::Text)).try_position()
            {
                panel.update(id(ButtonPartId::BarText), UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(font_size, info.name.clone())),
                    mix: Some(MixColorLch::color(primary_color)),
                    position: UiPosition::Absolute{position, align: UiPositionAlign::default()},
                    scissor_override: Some(bar_id),
                    ..UiElement::fit_content()
                });
            }
        }

        add_padding_horizontal(body, info.padding_left);

        body.update(id(ButtonPartId::Text), UiElement{
            texture: UiTexture::Text(TextInfo::new_simple(font_size, info.name)),
            mix: Some(MixColorLch::color(secondary_color)),
            ..UiElement::fit_content()
        });

        add_padding_horizontal(body, info.padding_right);

        inside_button && controls.take_click_down()
    }

    fn button_pad(parent: TreeInserter<MainMenuId>)
    {
        add_padding_vertical(parent, UiSize::Absolute(BUTTON_SIZE * 0.75).into())
    }

    fn update_main(
        &self,
        controls: &mut UiControls<MainMenuId>,
        menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let id = |part| MainMenuId::Main(part);

        let mut state = self.state;
        let mut action = MenuAction::None;

        add_padding_vertical(menu, UiSize::Rest(0.25).into());

        let title_font_size = (self.controller.screen_size().max() * 0.08) as u32;

        let title_outer = menu.update(id(MainPartId::Title(AlignPartId::Outer)), UiElement{
            children_layout: UiLayout::Horizontal,
            ..Default::default()
        });

        title_outer.update(id(MainPartId::Title(AlignPartId::Inner)), UiElement{
            texture: UiTexture::Text(TextInfo{
                font_size: title_font_size,
                text: TextBlocks::single(ACCENT_COLOR.into(), "stephanie".into()),
                outline: Some(TextOutline{color: [255; 3], size: 5})
            }),
            animation: Animation{
                scaling: Some(ScalingAnimation{
                    start_mode: Scaling::Spring(SpringScalingInfo{
                        start_velocity: vector![0.0, 1.0],
                        damping: 0.01,
                        strength: 100.0
                    }.into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..UiElement::fit_content()
        });

        add_padding_vertical(menu, UiSize::Rest(0.3).into());

        let buttons_panel_outer = menu.update(id(MainPartId::Buttons(AlignPartId::Outer)), UiElement{
            width: UiSize::Rest(1.0).into(),
            children_layout: UiLayout::Horizontal,
            ..Default::default()
        });

        let buttons_panel = buttons_panel_outer.update(id(MainPartId::Buttons(AlignPartId::Inner)), UiElement{
            width: UiSize::Rest(1.0).into(),
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        let button_pad = || Self::button_pad(buttons_panel);

        if self.update_main_button(controls, buttons_panel, |part| id(MainPartId::Start(part)), "start")
        {
            state = MenuState::WorldSelect;
        }

        button_pad();

        if self.update_main_button(controls, buttons_panel, |part| id(MainPartId::Options(part)), "options")
        {
            state = MenuState::Options;
        }

        button_pad();

        if self.update_main_button(controls, buttons_panel, |part| id(MainPartId::Quit(part)), "quit")
        {
            action = MenuAction::Quit;
        }

        add_padding_vertical(menu, UiSize::Rest(0.5).into());

        (state, action)
    }

    fn update_options(
        &self,
        controls: &mut UiControls<MainMenuId>,
        menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let id = |part| MainMenuId::Options(part);

        let mut state = self.state;

        let button_pad = || Self::button_pad(menu);

        add_padding_vertical(menu, UiSize::Rest(1.0).into());

        if self.update_main_button(controls, menu, |part| id(OptionsPartId::Controls(part)), "controls")
        {
            todo!();
        }

        button_pad();

        if self.update_main_button(controls, menu, |part| id(OptionsPartId::Back(part)), "back")
        {
            state = MenuState::Main;
        }

        add_padding_vertical(menu, UiSize::Rest(1.0).into());

        (state, MenuAction::None)
    }

    fn update_world_select(
        &self,
        controls: &mut UiControls<MainMenuId>,
        menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let id = |part| MainMenuId::WorldSelect(part);

        let mut state = self.state;
        let mut action = MenuAction::None;

        if self.worlds.is_empty()
        {
            let panel_padding = 0.05;
            let panel_padding_horizontal = 0.05;

            add_padding_vertical(menu, UiSize::Rest(1.0).into());

            let panel_outer = menu.update(id(WorldSelectPartId::Panel), UiElement{
                texture: UiTexture::Sliced(self.sliced_textures["rounded"]),
                mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                children_layout: UiLayout::Horizontal,
                ..Default::default()
            });

            add_padding_horizontal(panel_outer, panel_padding_horizontal.into());

            let panel = panel_outer.update(id(WorldSelectPartId::PanelInner), UiElement{
                children_layout: UiLayout::Vertical,
                ..Default::default()
            });

            add_padding_vertical(panel, panel_padding.into());

            let font_size = (self.controller.screen_size().max() * 0.02) as u32;
            panel.update(id(WorldSelectPartId::Message), UiElement{
                texture: UiTexture::Text(TextInfo::new_simple(font_size, "who am i")),
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                ..UiElement::fit_content()
            });

            add_padding_vertical(panel, 0.008.into());

            let textbox = panel.update(id(WorldSelectPartId::Textbox), UiElement{
                texture: UiTexture::Solid,
                width: UiSize::Rest(1.0).into(),
                height: TEXTBOX_SIZE.into(),
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                ..Default::default()
            });

            add_padding_vertical(panel, 0.01.into());

            let buttons = panel.update(id(WorldSelectPartId::Buttons), UiElement::default());

            let confirm_info = {
                let padding: UiElementSize<_> = 0.005.into();

                let font_height = self.fonts.text_height(font_size, self.controller.screen_size().max());

                ButtonInfo{
                    name: "confirm".to_owned(),
                    width: UiSize::FitChildren.into(),
                    height: font_height.into(),
                    body_width: UiSize::FitChildren.into(),
                    override_font_size: Some(font_size),
                    padding_left: padding.clone(),
                    padding_right: padding,
                    align_left: false,
                    invert_colors: true
                }
            };

            let confirm_clicked = self.update_button(
                controls,
                buttons,
                |part| id(WorldSelectPartId::Confirm(part)),
                confirm_info.clone()
            );

            let confirm_allowed = !self.info.name.is_empty();

            if confirm_clicked && confirm_allowed
            {
                action = MenuAction::Start(self.info.clone());
            }

            add_padding_horizontal(buttons, 0.005.into());

            let back_clicked = self.update_button(
                controls,
                buttons,
                |part| id(WorldSelectPartId::Back(part)),
                ButtonInfo{
                    name: "back".to_owned(),
                    ..confirm_info
                }
            );

            if back_clicked
            {
                state = MenuState::Main;
            }

            add_padding_vertical(panel, panel_padding.into());
            add_padding_horizontal(panel_outer, panel_padding_horizontal.into());

            add_padding_vertical(menu, UiSize::Rest(1.0).into());
        } else
        {
            todo!();
        }

        (state, action)
    }

    pub fn update<'a>(
        &mut self,
        partial_info: UpdateBuffersPartialInfo<'a>,
        dt: f32
    ) -> (UpdateBuffersPartialInfo<'a>, MenuAction)
    {
        let mut controls = self.controls.changed_this_frame();

        let align_left = match self.state
        {
            MenuState::Main => true,
            MenuState::Options => true,
            MenuState::WorldSelect => false
        };

        self.controller.as_inserter().element().children_layout = if align_left
        {
            add_padding_horizontal(self.controller.as_inserter(), 0.08.into());

            UiLayout::Horizontal
        } else
        {
            UiLayout::Vertical
        };

        let menu = self.controller.update(MainMenuId::Menu, UiElement{
            height: UiSize::Rest(1.0).into(),
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        let (next_state, action) = match self.state
        {
            MenuState::Main => self.update_main(&mut controls, menu),
            MenuState::Options => self.update_options(&mut controls, menu),
            MenuState::WorldSelect => self.update_world_select(&mut controls, menu)
        };

        self.state = next_state;

        let mut info = partial_info.to_full(&self.ui_camera);

        self.controller.create_renders(&mut info, dt);
        self.controller.update_buffers(&mut info);

        info.with_projection(Matrix4::identity(), |info|
        {
            self.screen_object.update_buffers(info);
        });

        self.controls.consume_changed(controls).for_each(drop);

        (info.partial, action)
    }

    pub fn input(&mut self, control: Control)
    {
        self.controls.handle_input(control);
    }

    pub fn mouse_move(&mut self, (x, y): (f64, f64))
    {
        let normalized_size = self.ui_camera.normalized_size();
        let position = vector![x as f32, y as f32].component_mul(&normalized_size) - (normalized_size / 2.0);

        self.controller.set_mouse_position(position);
    }

    pub fn draw(&mut self, mut info: DrawInfo)
    {
        info.next_subpass();
        info.next_subpass();

        info.next_subpass();

        info.bind_pipeline(self.shaders.menu_background);

        self.screen_object.draw(&mut info);

        info.next_subpass();

        info.bind_pipeline(self.shaders.ui);

        self.controller.draw(&mut info);
    }

    pub fn resize(&mut self, aspect: f32)
    {
        self.ui_camera.resize(aspect);
    }
}
