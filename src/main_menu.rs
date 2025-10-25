use std::{
    rc::Rc,
    collections::HashMap
};

use nalgebra::{vector, Matrix4};

use vulkano::descriptor_set::WriteDescriptorSet;

use yanyaengine::{game_object::*, Control, SolidObject, camera::Camera};

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
    Title,
    Buttons,
    Start(ButtonPartId),
    Options(ButtonPartId),
    Quit(ButtonPartId)
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
enum ButtonPartId
{
    Panel,
    Text,
    Body,
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
    Options
}

const BUTTON_SIZE: f32 = 0.05;

#[derive(Clone)]
pub struct MenuClientInfo
{
    pub address: Option<String>,
    pub name: String,
    pub host: bool,
    pub debug: bool
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

        Self{
            shaders,
            sliced_textures,
            screen_object: client::create_screen_object(partial_info),
            controller,
            controls: ControlsController::new(),
            state: MenuState::Main,
            ui_camera,
            info
        }
    }

    fn update_button(
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

        let mix_animation = MixAnimation{
            decay: MixDecay::all(30.0),
            ..Default::default()
        };

        let button_height = panel.try_height().unwrap_or(BUTTON_SIZE);

        let body = panel.update(id(ButtonPartId::Body), UiElement{
            texture: UiTexture::Sliced(self.sliced_textures["rounded"]),
            mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
            width: UiSize::Absolute(button_height * 5.0).into(),
            height: UiSize::Rest(1.0).into(),
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

        let outline_body = body.update(id(ButtonPartId::Outline), UiElement{
            texture: UiTexture::Sliced(self.sliced_textures["rounded_outline"]),
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
                texture: UiTexture::Text{text: name.to_owned(), font_size},
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
            let mut element = body.element();
            element.width = UiSize::Absolute(button_height * 6.0).into();
            element.mix = Some(MixColorLch::color(ACCENT_COLOR));
        } else
        {
            body.element().animation.scaling.as_mut().unwrap().start_mode = Scaling::Spring(SpringScalingInfo{
                start_velocity: vector![0.0, 0.0],
                damping: 0.00001,
                strength: 230.0
            }.into());
        }

        inside_button && controls.take_click_down()
    }

    fn update_main(
        &self,
        controls: &mut UiControls<MainMenuId>,
        menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let mut state = self.state;
        let mut action = MenuAction::None;

        add_padding_vertical(menu, UiSize::Rest(0.25).into());

        let title_size = 0.15;

        let title_texture = UiTexture::Custom("ui/title.png".into());
        let aspect = {
            let size = self.controller.texture_size(&title_texture);

            size.x / size.y
        };

        menu.update(MainMenuId::Title, UiElement{
            texture: title_texture,
            width: UiSize::Absolute(title_size * aspect).into(),
            height: UiSize::Absolute(title_size).into(),
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
            ..Default::default()
        });

        add_padding_vertical(menu, UiSize::Rest(0.3).into());

        let buttons_panel = menu.update(MainMenuId::Buttons, UiElement{
            width: UiSize::Rest(1.0).into(),
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        let button_pad = || add_padding_vertical(buttons_panel, UiSize::Absolute(BUTTON_SIZE * 0.75).into());

        if self.update_button(controls, buttons_panel, |part| MainMenuId::Start(part), "start")
        {
            action = MenuAction::Start(self.info.clone());
        }

        button_pad();

        if self.update_button(controls, buttons_panel, |part| MainMenuId::Options(part), "options")
        {
            state = MenuState::Options;
        }

        button_pad();

        if self.update_button(controls, buttons_panel, |part| MainMenuId::Quit(part), "quit")
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
        (self.state, MenuAction::None)
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
            MenuState::Options => self.update_options(&mut controls, menu)
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

        info.bind_pipeline(self.shaders.final_mix);
        info.current_sets = vec![info.create_descriptor_set(0, [
            WriteDescriptorSet::image_view(0, info.attachments[0].clone()),
            WriteDescriptorSet::image_view(1, info.attachments[2].clone()),
            WriteDescriptorSet::image_view(2, info.attachments[4].clone())
        ])];

        self.screen_object.draw(&mut info);

        info.next_subpass();

        info.current_sets.clear();

        info.bind_pipeline(self.shaders.ui);

        self.controller.draw(&mut info);
    }

    pub fn resize(&mut self, aspect: f32)
    {
        self.ui_camera.resize(aspect);
    }
}
