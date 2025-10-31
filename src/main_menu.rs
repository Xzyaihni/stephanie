use std::{
    fs,
    path::PathBuf,
    rc::Rc,
    collections::HashMap
};

use nalgebra::{vector, Matrix4};

use yanyaengine::{game_object::*, Control, SolidObject, camera::Camera};

use crate::{
    app::ProgramShaders,
    client::{
        self,
        SlicedTexture,
        game_state::{
            UiControls,
            ControlsController,
            ui::{BACKGROUND_COLOR, ACCENT_COLOR, HIGHLIGHTED_COLOR, controller::*}
        }
    },
    common::{
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
    Controls(ControlsPartId),
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
    Title,
    Buttons,
    Start(ButtonPartId),
    Options(ButtonPartId),
    Quit(ButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum OptionsPartId
{
    Controls(ButtonPartId),
    Back(ButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ControlsPartId
{
    Back(ButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WorldSelectPartId
{
    Textbox(OutlinePart),
    Buttons,
    Confirm(ButtonPartId),
    Back(ButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ButtonPartId
{
    Panel,
    Text,
    Body(OutlinePart)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum OutlinePart
{
    Normal,
    Outline
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
    Controls,
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

struct ButtonInfo
{
    name: String,
    body_texture: SlicedTexture,
    outline_texture: SlicedTexture,
    width: UiElementSize<MainMenuId>,
    height: UiElementSize<MainMenuId>
}

struct ButtonResult<'a>
{
    body: TreeInserter<'a, MainMenuId>,
    clicked: bool,
    inside: bool
}

pub struct MainMenu
{
    shaders: ProgramShaders,
    sliced_textures: Rc<HashMap<String, SlicedTexture>>,
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
        let panel = parent.update(id(ButtonPartId::Panel), UiElement{
            width: UiSize::Rest(1.0).into(),
            height: UiElementSize{
                minimum_size: Some(UiMinimumSize::Pixels(85.0)),
                size: UiSize::Absolute(BUTTON_SIZE)
            },
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        let button_height = panel.try_height().unwrap_or(BUTTON_SIZE);

        let result = self.update_button(controls, panel, id, ButtonInfo{
            name: name.to_owned(),
            body_texture: self.sliced_textures["big_rounded"],
            outline_texture: self.sliced_textures["big_rounded_outline"],
            width: UiSize::Absolute(button_height * 5.0).into(),
            height: UiSize::Rest(1.0).into()
        });

        if result.inside
        {
            result.body.element().width = UiSize::Absolute(button_height * 6.0).into();
        } else
        {
            result.body.element().animation.scaling.as_mut().unwrap().start_mode = Scaling::Spring(SpringScalingInfo{
                start_velocity: vector![0.0, 0.0],
                damping: 0.00001,
                strength: 230.0
            }.into());
        }

        result.clicked
    }

    fn update_button<'a>(
        &self,
        controls: &mut UiControls<MainMenuId>,
        parent: TreeInserter<'a, MainMenuId>,
        id: impl Fn(ButtonPartId) -> MainMenuId,
        info: ButtonInfo
    ) -> ButtonResult<'a>
    {
        let mix_animation = MixAnimation{
            decay: MixDecay::all(30.0),
            ..Default::default()
        };

        let body = parent.update(id(ButtonPartId::Body(OutlinePart::Normal)), UiElement{
            texture: UiTexture::Sliced(info.body_texture),
            mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
            width: info.width,
            height: info.height,
            animation: Animation{
                scaling: Some(ScalingAnimation{
                    start_scaling: vector![0.0, 1.0],
                    start_mode: Scaling::EaseOut{decay: 16.0},
                    ..Default::default()
                }),
                mix: Some(mix_animation.clone()),
                ..Default::default()
            },
            ..Default::default()
        });

        let inside_button = body.is_mouse_inside();

        let text_mix = MixColorLch::color(if inside_button { BACKGROUND_COLOR } else { ACCENT_COLOR });

        let outline_body = body.update(id(ButtonPartId::Body(OutlinePart::Outline)), UiElement{
            texture: UiTexture::Sliced(info.outline_texture),
            mix: Some(MixColorLch::color(ACCENT_COLOR)),
            width: UiSize::Rest(1.0).into(),
            height: UiSize::Rest(1.0).into(),
            animation: Animation{
                mix: Some(mix_animation.clone()),
                ..Default::default()
            },
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        if let Some(height) = body.try_height()
        {
            let font_size = (self.controller.screen_size().max() * height * 0.6) as u32;
            outline_body.update(id(ButtonPartId::Text), UiElement{
                texture: UiTexture::Text(TextInfo::new_simple(font_size, info.name)),
                mix: Some(text_mix),
                animation: Animation{
                    mix: Some(mix_animation),
                    ..Default::default()
                },
                inherit_animation: false,
                ..UiElement::fit_content()
            });
        }

        if inside_button
        {
            body.element().mix = Some(MixColorLch::color(ACCENT_COLOR));
        }

        ButtonResult{
            body,
            inside: inside_button,
            clicked: inside_button && controls.take_click_down()
        }
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

        menu.update(id(MainPartId::Title), UiElement{
            texture: UiTexture::Text(TextInfo{
                font_size: title_font_size,
                text: TextBlocks::single(HIGHLIGHTED_COLOR.into(), "stephanie".into()),
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

        let buttons_panel = menu.update(id(MainPartId::Buttons), UiElement{
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
            state = MenuState::Controls;
        }

        button_pad();

        if self.update_main_button(controls, menu, |part| id(OptionsPartId::Back(part)), "back")
        {
            state = MenuState::Main;
        }

        add_padding_vertical(menu, UiSize::Rest(1.0).into());

        (state, MenuAction::None)
    }

    fn update_controls(
        &self,
        controls: &mut UiControls<MainMenuId>,
        menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let id = |part| MainMenuId::Controls(part);

        let mut state = self.state;

        add_padding_vertical(menu, UiSize::Rest(1.0).into());

        if self.update_main_button(controls, menu, |part| id(ControlsPartId::Back(part)), "back")
        {
            state = MenuState::Options;
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
            add_padding_vertical(menu, UiSize::Rest(1.0).into());

            let textbox_width = TEXTBOX_SIZE * 5.0;
            let textbox = menu.update(id(WorldSelectPartId::Textbox(OutlinePart::Normal)), UiElement{
                texture: UiTexture::Sliced(self.sliced_textures["rounded"]),
                width: textbox_width.into(),
                height: TEXTBOX_SIZE.into(),
                mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                ..Default::default()
            });

            textbox.update(id(WorldSelectPartId::Textbox(OutlinePart::Outline)), UiElement{
                texture: UiTexture::Sliced(self.sliced_textures["rounded_outline"]),
                width: UiSize::Rest(1.0).into(),
                height: UiSize::Rest(1.0).into(),
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                ..Default::default()
            });

            add_padding_vertical(menu, 0.01.into());

            let buttons = menu.update(id(WorldSelectPartId::Buttons), UiElement{
                width: textbox_width.into(),
                height: (TEXTBOX_SIZE * 0.75).into(),
                ..Default::default()
            });

            let confirm_clicked = self.update_button(
                controls,
                buttons,
                |part| id(WorldSelectPartId::Confirm(part)),
                ButtonInfo{
                    name: "confirm".to_owned(),
                    body_texture: self.sliced_textures["rounded"],
                    outline_texture: self.sliced_textures["rounded_outline"],
                    width: UiSize::Rest(1.0).into(),
                    height: UiSize::Rest(1.0).into()
                }
            ).clicked;

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
                    body_texture: self.sliced_textures["rounded"],
                    outline_texture: self.sliced_textures["rounded_outline"],
                    width: UiSize::Rest(1.0).into(),
                    height: UiSize::Rest(1.0).into()
                }
            ).clicked;

            if back_clicked
            {
                state = MenuState::Main;
            }

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

        let menu = self.controller.update(MainMenuId::Menu, UiElement{
            width: UiSize::Rest(1.0).into(),
            height: UiSize::Rest(1.0).into(),
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        let (next_state, action) = match self.state
        {
            MenuState::Main => self.update_main(&mut controls, menu),
            MenuState::Options => self.update_options(&mut controls, menu),
            MenuState::Controls => self.update_controls(&mut controls, menu),
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
