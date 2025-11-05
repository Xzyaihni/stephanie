use std::{
    f32,
    fs,
    borrow::Cow,
    path::PathBuf,
    rc::Rc,
    collections::HashMap
};

use nalgebra::{vector, Vector2, Matrix4};

use strum::IntoEnumIterator;

use yanyaengine::{
    game_object::*,
    KeyCode,
    FontsContainer,
    Control,
    SolidObject,
    camera::Camera
};

use crate::{
    app::ProgramShaders,
    client::{
        self,
        ui_common::*,
        SlicedTexture,
        game_state::{
            KeyMapping,
            UiControls,
            ControlsController,
            Control as GameControl,
            default_bindings
        }
    },
    common::{
        sanitized_name,
        from_upper_camel,
        some_or_value,
        render_info::*,
        colors::Lcha,
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
    WorldSelect(WorldSelectPartId),
    WorldCreate(WorldCreatePartId)
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
    Menu,
    Controls(ButtonPartId),
    DebugToggle(ButtonPartId),
    Back(ButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ControlsPartId
{
    PanelOuter,
    PanelBetween,
    Title,
    TitleText,
    Separator(SeparatorPartId),
    Panel,
    Buttons,
    Back(ButtonPartId),
    List(UiListPart),
    Button(GameControl, ControlButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ControlButtonPartId
{
    Body,
    Text,
    Binding
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SeparatorPartId
{
    Outer,
    Body
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WorldSelectPartId
{
    PanelOuter,
    Panel,
    List(UiListPart),
    Button(usize, WorldButtonPartId),
    Buttons,
    Create(ButtonPartId),
    Back(ButtonPartId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WorldCreatePartId
{
    Panel,
    PanelInner,
    Message,
    Textbox(TextboxPartId),
    Buttons,
    Confirm(ButtonPartId),
    Back(ButtonPartId)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WorldButtonPartId
{
    Body,
    Text
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
    Rebind(GameControl, KeyMapping),
    Quit,
    Start
}

#[derive(Clone, Copy)]
enum MenuState
{
    Main,
    Options,
    Controls,
    WorldSelect,
    WorldCreate
}

const BUTTON_SIZE: f32 = 0.05;

#[derive(Clone)]
pub struct MenuClientInfo
{
    pub address: Option<String>,
    pub name: TextboxInfo,
    pub host: bool,
    pub debug: bool
}

struct WorldInfo
{
    name: String,
    path: PathBuf,
    id: usize
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
    disabled: bool,
    invert_colors: bool
}

struct Binding
{
    pub control: GameControl,
    pub mapping: Option<KeyMapping>,
    pub duplicate: bool
}

struct UiInfo<'a, 'b, 'c, 'd, 'e, 'f>
{
    controls: &'b mut UiControls<MainMenuId>,
    bindings: &'e mut UiList<Binding>,
    sliced_textures: &'a HashMap<String, SlicedTexture>,
    fonts: &'a FontsContainer,
    info: &'c mut MenuClientInfo,
    worlds: &'d mut UiList<WorldInfo>,
    controls_taken: &'f mut Option<GameControl>,
    state: MenuState,
    dt: f32
}

pub struct MainMenu
{
    pub info: MenuClientInfo,
    shaders: ProgramShaders,
    sliced_textures: Rc<HashMap<String, SlicedTexture>>,
    fonts: Rc<FontsContainer>,
    screen_object: SolidObject,
    controller: Controller<MainMenuId>,
    controls: ControlsController<MainMenuId>,
    state: MenuState,
    ui_camera: Camera,
    controls_taken: Option<GameControl>,
    bindings: UiList<Binding>,
    worlds: UiList<WorldInfo>
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
            name: TextboxInfo::new_with_limit("stephanie".to_owned(), 30),
            host: true,
            debug: false
        };

        let ui_camera = Camera::new(partial_info.aspect(), -1.0..1.0);

        let worlds_path = PathBuf::from("worlds");
        let mut worlds = if worlds_path.exists()
        {
            fs::read_dir(worlds_path).and_then(|iter| -> Result<Vec<_>, _>
            {
                iter.enumerate().map(|(id, x)|
                {
                    x.map(|x|
                    {
                        WorldInfo{
                            name: x.file_name().to_string_lossy().into_owned(),
                            path: x.path(),
                            id
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

        let current_bindings = default_bindings();

        let mut bindings = GameControl::iter().map(|control|
        {
            Binding{
                control,
                mapping: current_bindings.iter().find(|(_, check)| *check == control).map(|(x, _)| *x),
                duplicate: false
            }
        }).collect::<Vec<_>>();

        bindings.sort_by(|a, b| a.control.cmp(&b.control));

        let controls = ControlsController::new(current_bindings);

        let mut this = Self{
            shaders,
            sliced_textures,
            fonts: partial_info.builder_wrapper.fonts().clone(),
            screen_object: client::create_screen_object(partial_info),
            controller,
            controls,
            state: MenuState::Main,
            ui_camera,
            controls_taken: None,
            bindings: bindings.into(),
            worlds: worlds.into(),
            info
        };

        this.check_duplicates();

        this
    }

    pub fn rebind(&mut self, control: GameControl, key: KeyMapping)
    {
        if let Some(binding) = self.bindings.items.iter_mut().find(|x| x.control == control)
        {
            binding.mapping = Some(key);

            self.check_duplicates();
        }
    }

    fn check_duplicates(&mut self)
    {
        for i in 0..self.bindings.items.len()
        {
            let this_key = self.bindings.items[i].mapping;
            let is_duplicate = this_key.map(|key|
            {
                self.bindings.items.iter().enumerate().any(|(index, x)|
                {
                    if index == i
                    {
                        return false;
                    }

                    x.mapping.map(|x| x == key).unwrap_or(false)
                })
            }).unwrap_or(false);

            self.bindings.items[i].duplicate = is_duplicate;
        }
    }

    pub fn bindings(&self) -> Vec<(KeyMapping, GameControl)>
    {
        self.bindings.items.iter().filter_map(|x|
        {
            x.mapping.map(|key| (key, x.control))
        }).collect()
    }

    fn update_main_button(
        controls: &mut UiControls<MainMenuId>,
        parent: TreeInserter<MainMenuId>,
        id: impl Fn(ButtonPartId) -> MainMenuId,
        name: &str
    ) -> bool
    {
        Self::update_button(controls, parent, id, ButtonInfo{
            name: name.to_owned(),
            width: UiElementSize{
                minimum_size: Some(UiMinimumSize::FitChildren),
                size: UiSize::Rest(1.0)
            },
            height: UiElementSize{
                minimum_size: Some(UiMinimumSize::Pixels(85.0)),
                size: UiSize::Absolute(BUTTON_SIZE)
            },
            body_width: UiSize::FitChildren.into(),
            override_font_size: None,
            padding_left: 0.013.into(),
            padding_right: 0.01.into(),
            align_left: true,
            disabled: false,
            invert_colors: false
        })
    }

    fn update_button(
        controls: &mut UiControls<MainMenuId>,
        parent: TreeInserter<MainMenuId>,
        id: impl Fn(ButtonPartId) -> MainMenuId,
        info: ButtonInfo
    ) -> bool
    {
        let colors@(primary_color, secondary_color) = if info.disabled
        {
            (ACCENT_COLOR_FADED, BACKGROUND_COLOR)
        } else
        {
            (ACCENT_COLOR, BACKGROUND_COLOR)
        };

        let (primary_color, secondary_color) = if info.invert_colors
        {
            (secondary_color, primary_color)
        } else
        {
            colors
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

        let inside_button = body.is_mouse_inside() && !info.disabled;

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
            (body.screen_size().max() * height * 0.6) as u32
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

        if body.input_of(&bar_id).exists()
        {
            if let Some(position) = body.input_of(&id(ButtonPartId::Text)).try_position()
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
        ui_info: UiInfo,
        menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let id = |part| MainMenuId::Main(part);

        let mut state = ui_info.state;
        let mut action = MenuAction::None;

        add_padding_vertical(menu, UiSize::Rest(0.25).into());

        let title_font_size = (menu.screen_size().max() * 0.08) as u32;

        let title_outer = menu.update(id(MainPartId::Title(AlignPartId::Outer)), UiElement{
            children_layout: UiLayout::Horizontal,
            ..Default::default()
        });

        let title_text = title_outer.update(id(MainPartId::Title(AlignPartId::Inner)), UiElement{
            texture: UiTexture::Text(TextInfo{
                font_size: title_font_size,
                text: TextBlocks::single(ACCENT_COLOR.into(), "stephanie".into()),
                outline: Some(TextOutline{color: BACKGROUND_COLOR.into(), size: 5})
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

        let buttons_panel = {
            let width = match buttons_panel_outer.try_width()
            {
                Some(x) => x,
                None =>
                {
                    title_text.element().texture = UiTexture::None;

                    return (state, action);
                }
            };

            buttons_panel_outer.update(id(MainPartId::Buttons(AlignPartId::Inner)), UiElement{
                width: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Vertical,
                animation: Animation{
                    position: Some(PositionAnimation{
                        offsets: Some(PositionOffsets{
                            start: vector![-width, 0.0],
                            ..Default::default()
                        }),
                        start_mode: Connection::EaseOut{decay: 16.0, limit: None},
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            })
        };

        let button_pad = || Self::button_pad(buttons_panel);

        if Self::update_main_button(ui_info.controls, buttons_panel, |part| id(MainPartId::Start(part)), "start")
        {
            state = MenuState::WorldSelect;
        }

        button_pad();

        if Self::update_main_button(ui_info.controls, buttons_panel, |part| id(MainPartId::Options(part)), "options")
        {
            state = MenuState::Options;
        }

        button_pad();

        if Self::update_main_button(ui_info.controls, buttons_panel, |part| id(MainPartId::Quit(part)), "quit")
        {
            action = MenuAction::Quit;
        }

        add_padding_vertical(menu, UiSize::Rest(0.5).into());

        (state, action)
    }

    fn update_options(
        ui_info: UiInfo,
        outer_menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let id = |part| MainMenuId::Options(part);

        let mut state = ui_info.state;

        let menu = {
            let width = outer_menu.try_width().unwrap_or(0.0);

            outer_menu.update(id(OptionsPartId::Menu), UiElement{
                width: UiSize::Rest(1.0).into(),
                height: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Vertical,
                    animation: Animation{
                    position: Some(PositionAnimation{
                        offsets: Some(PositionOffsets{
                            start: vector![-width, 0.0],
                            ..Default::default()
                        }),
                        start_mode: Connection::EaseOut{decay: 16.0, limit: None},
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            })
        };

        let button_pad = || Self::button_pad(menu);

        add_padding_vertical(menu, UiSize::Rest(1.0).into());

        if Self::update_main_button(ui_info.controls, menu, |part| id(OptionsPartId::Controls(part)), "controls")
        {
            state = MenuState::Controls;
        }

        button_pad();

        let debug_mode_text = format!("debug mode: {}", if ui_info.info.debug { "on" } else { "off" });
        if Self::update_main_button(ui_info.controls, menu, |part| id(OptionsPartId::DebugToggle(part)), &debug_mode_text)
        {
            ui_info.info.debug = !ui_info.info.debug;
        }

        button_pad();

        if Self::update_main_button(ui_info.controls, menu, |part| id(OptionsPartId::Back(part)), "back")
        {
            state = MenuState::Main;
        }

        add_padding_vertical(menu, UiSize::Rest(1.0).into());

        (state, MenuAction::None)
    }

    fn update_controls(
        ui_info: UiInfo,
        outer_menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let id = |part| MainMenuId::Controls(part);

        let mut state = ui_info.state;
        let mut action = MenuAction::None;

        if let MenuState::Controls = state
        {
            let panel_outer = outer_menu.update(id(ControlsPartId::PanelOuter), UiElement{
                height: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Vertical,
                ..Default::default()
            });

            add_padding_vertical(panel_outer, 0.02.into());

            let font_size = MEDIUM_TEXT_SIZE;
            let item_height = ui_info.fonts.text_height(font_size, panel_outer.screen_size().max());

            let panel_between = panel_outer.update(id(ControlsPartId::PanelBetween), UiElement{
                texture: UiTexture::Sliced(ui_info.sliced_textures["rounded"]),
                mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                height: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Vertical,
                animation: Animation{
                    position: Some(PositionAnimation{
                        offsets: Some(PositionOffsets{
                            start: vector![0.0, -1.0],
                            ..Default::default()
                        }),
                        start_mode: Connection::EaseOut{decay: 16.0, limit: None},
                        ..Default::default()
                    }),
                    mix: Some(MixAnimation{
                        start_mix: Some(Lcha{a: 0.0, ..BACKGROUND_COLOR}),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            });

            let title = panel_between.update(id(ControlsPartId::Title), UiElement{
                children_layout: UiLayout::Vertical,
                ..Default::default()
            });

            title.update(id(ControlsPartId::TitleText), UiElement{
                texture: UiTexture::Text(TextInfo::new_simple(font_size, "controls rebinding")),
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                ..UiElement::fit_content()
            });

            add_padding_vertical(panel_between, UiSize::Pixels(TINY_PADDING).into());

            let outer_separator = panel_between.update(id(ControlsPartId::Separator(SeparatorPartId::Outer)), UiElement{
                width: UiSize::Rest(1.0).into(),
                height: UiSize::Pixels(SEPARATOR_SIZE).into(),
                ..Default::default()
            });

            add_padding_horizontal(outer_separator, UiSize::Pixels(MEDIUM_PADDING).into());
            outer_separator.update(id(ControlsPartId::Separator(SeparatorPartId::Body)), UiElement{
                texture: UiTexture::Solid,
                mix: Some(MixColorLch::color(ACCENT_COLOR)),
                width: UiSize::Rest(1.0).into(),
                height: UiSize::Rest(1.0).into(),
                ..Default::default()
            });
            add_padding_horizontal(outer_separator, UiSize::Pixels(MEDIUM_PADDING).into());

            add_padding_vertical(panel_between, UiSize::Pixels(TINY_PADDING).into());

            let panel = panel_between.update(id(ControlsPartId::Panel), UiElement{
                height: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Horizontal,
                ..Default::default()
            });

            add_padding_horizontal(panel, 0.03.into());

            let list_info = UiListInfo{
                controls: ui_info.controls,
                mouse_taken: false,
                item_height,
                padding: SMALL_PADDING,
                outer_width: UiSize::Pixels(600.0).into(),
                outer_height: UiSize::Rest(1.0).into(),
                dt: ui_info.dt
            };

            ui_info.bindings.update(panel, |part| id(ControlsPartId::List(part)), list_info, |info, parent, item, is_selected|
            {
                let id = |part| id(ControlsPartId::Button(item.control, part));

                let body = parent.update(id(ControlButtonPartId::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(if is_selected { ACCENT_COLOR } else { BACKGROUND_COLOR })),
                    width: UiElementSize{
                        minimum_size: Some(UiMinimumSize::FitChildren),
                        size: UiSize::Rest(1.0)
                    },
                    height: item_height.into(),
                    animation: Animation{
                        mix: Some(MixAnimation::default()),
                        ..Default::default()
                    },
                    children_layout: UiLayout::Horizontal,
                    ..Default::default()
                });

                add_padding_horizontal(body, 0.005.into());

                body.update(id(ControlButtonPartId::Text), UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(font_size, item.control.name())),
                    mix: Some(MixColorLch::color(if is_selected { BACKGROUND_COLOR } else { ACCENT_COLOR })),
                    animation: Animation{
                        mix: Some(MixAnimation::default()),
                        ..Default::default()
                    },
                    ..UiElement::fit_content()
                });

                add_padding_horizontal(body, UiSize::Rest(1.0).into());

                let binding_name: Cow<'static, str> = item.mapping.map(|x|
                {
                    from_upper_camel(&x.to_string()).into()
                }).unwrap_or_else(|| "unbound".into());

                let binding_text = body.update(id(ControlButtonPartId::Binding), UiElement{
                    texture: UiTexture::Text(TextInfo{
                        font_size,
                        text: TextBlocks::single((if is_selected { BACKGROUND_COLOR } else { ACCENT_COLOR }).into(), binding_name),
                        outline: item.duplicate.then(|| TextOutline{color: SPECIAL_COLOR_TWO.into(), size: 2})
                    }),
                    height: UiSize::Rest(1.0).into(),
                    ..UiElement::fit_content()
                });

                add_padding_horizontal(body, 0.005.into());

                if is_selected && info.controls.take_click_down()
                {
                    *ui_info.controls_taken = Some(item.control);
                }

                if *ui_info.controls_taken == Some(item.control)
                {
                    binding_text.element().texture = UiTexture::Text(TextInfo::new_simple(font_size, "..."));
                }
            });

            add_padding_horizontal(panel, 0.03.into());

            let button_panel = panel_between.update(id(ControlsPartId::Buttons), UiElement{
                width: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Vertical,
                ..Default::default()
            });

            let back_clicked = {
                let padding: UiElementSize<_> = 0.005.into();

                Self::update_button(
                    ui_info.controls,
                    button_panel,
                    |part| id(ControlsPartId::Back(part)),
                    ButtonInfo{
                        name: "back".to_owned(),
                        width: UiSize::FitChildren.into(),
                        height: item_height.into(),
                        body_width: UiSize::FitChildren.into(),
                        override_font_size: Some(font_size),
                        padding_left: padding.clone(),
                        padding_right: padding,
                        align_left: false,
                        disabled: false,
                        invert_colors: true
                    }
                )
            };

            if back_clicked
            {
                state = MenuState::Options;
            }

            add_padding_vertical(panel, UiSize::Pixels(SMALL_PADDING).into());

            add_padding_vertical(panel_outer, 0.02.into());
        }

        if let Some(control) = ui_info.controls_taken
        {
            if let Some((key, _)) = ui_info.controls.controls.iter().find(|(_key, state)| state.is_down())
            {
                action = MenuAction::Rebind(*control, key.key);
                ui_info.controls_taken.take();
            }
        }

        (state, action)
    }

    fn update_world_create(
        ui_info: UiInfo,
        menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let id = |part| MainMenuId::WorldCreate(part);

        let mut state = ui_info.state;
        let mut action = MenuAction::None;

        let panel_padding = 0.05;
        let panel_padding_horizontal = 0.06;

        let panel_outer = menu.update(id(WorldCreatePartId::Panel), UiElement{
            texture: UiTexture::Sliced(ui_info.sliced_textures["rounded"]),
            mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
            position: UiPosition::Absolute{position: Vector2::zeros(), align: UiPositionAlign::default()},
            children_layout: UiLayout::Horizontal,
            ..Default::default()
        });

        add_padding_horizontal(panel_outer, panel_padding_horizontal.into());

        let panel = panel_outer.update(id(WorldCreatePartId::PanelInner), UiElement{
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        add_padding_vertical(panel, (panel_padding * 0.8).into());

        let font_size = (panel.screen_size().max() * 0.02) as u32;
        let font_height = ui_info.fonts.text_height(font_size, panel.screen_size().max());

        panel.update(id(WorldCreatePartId::Message), UiElement{
            texture: UiTexture::Text(TextInfo::new_simple(font_size, "who am i?")),
            mix: Some(MixColorLch::color(ACCENT_COLOR)),
            ..UiElement::fit_content()
        });

        add_padding_vertical(panel, 0.008.into());

        let textbox = panel.update(id(WorldCreatePartId::Textbox(TextboxPartId::Body)), UiElement{
            width: UiElementSize{
                minimum_size: Some(UiMinimumSize::FitChildren),
                size: UiSize::Rest(1.0)
            },
            children_layout: UiLayout::Vertical,
            ..Default::default()
        });

        ui_info.info.name.update(ui_info.dt);

        textbox_update(
            ui_info.controls,
            ui_info.fonts,
            |part| MainMenuId::WorldCreate(WorldCreatePartId::Textbox(part)),
            textbox,
            font_size,
            &mut ui_info.info.name
        );

        add_padding_vertical(panel, 0.01.into());

        let buttons = panel.update(id(WorldCreatePartId::Buttons), UiElement::default());

        let confirm_allowed = !ui_info.info.name.text.is_empty();

        let confirm_info = {
            let padding: UiElementSize<_> = 0.005.into();

            ButtonInfo{
                name: "confirm".to_owned(),
                width: UiSize::FitChildren.into(),
                height: font_height.into(),
                body_width: UiSize::FitChildren.into(),
                override_font_size: Some(font_size),
                padding_left: padding.clone(),
                padding_right: padding,
                align_left: false,
                disabled: !confirm_allowed,
                invert_colors: true
            }
        };

        let back_clicked = Self::update_button(
            ui_info.controls,
            buttons,
            |part| id(WorldCreatePartId::Back(part)),
            ButtonInfo{
                name: "back".to_owned(),
                disabled: false,
                ..confirm_info.clone()
            }
        );

        if back_clicked
        {
            state = MenuState::Main;
        }

        add_padding_horizontal(buttons, 0.005.into());

        let confirm_clicked = Self::update_button(
            ui_info.controls,
            buttons,
            |part| id(WorldCreatePartId::Confirm(part)),
            confirm_info
        );

        let confirm_clicked = confirm_clicked || ui_info.controls.take_key_down(KeyMapping::Keyboard(KeyCode::Enter));

        if confirm_clicked && confirm_allowed
        {
            action = MenuAction::Start;
        }

        add_padding_vertical(panel, panel_padding.into());
        add_padding_horizontal(panel_outer, panel_padding_horizontal.into());

        if let MenuAction::Start = action
        {
            fn unique_name(worlds: &[WorldInfo], name: String) -> String
            {
                if worlds.iter().any(|x| x.name == name)
                {
                    unique_name(worlds, name + "_")
                } else
                {
                    name
                }
            }

            let world_name = sanitized_name(&ui_info.info.name.text);

            ui_info.info.name.text = unique_name(&ui_info.worlds.items, world_name);
        }

        (state, action)
    }

    fn update_world_select(
        ui_info: UiInfo,
        menu: TreeInserter<MainMenuId>
    ) -> (MenuState, MenuAction)
    {
        let id = |part| MainMenuId::WorldSelect(part);

        let mut state = ui_info.state;
        let mut action = MenuAction::None;

        if ui_info.worlds.items.is_empty()
        {
            state = MenuState::WorldCreate;
        } else
        {
            add_padding_vertical(menu, 0.05.into());

            let outer_panel = menu.update(id(WorldSelectPartId::PanelOuter), UiElement{
                texture: UiTexture::Sliced(ui_info.sliced_textures["rounded"]),
                mix: Some(MixColorLch::color(BACKGROUND_COLOR)),
                width: UiSize::FitChildren.into(),
                height: UiSize::Rest(1.0).into(),
                children_layout: UiLayout::Vertical,
                ..Default::default()
            });

            add_padding_vertical(outer_panel, UiSize::Pixels(SMALL_PADDING).into());

            let panel = outer_panel.update(id(WorldSelectPartId::Panel), UiElement{
                width: UiSize::FitChildren.into(),
                height: UiSize::Rest(1.0).into(),
                ..Default::default()
            });

            let font_size = MEDIUM_TEXT_SIZE;
            let item_height = ui_info.fonts.text_height(font_size, menu.screen_size().max());

            let list_info = UiListInfo{
                controls: ui_info.controls,
                mouse_taken: false,
                item_height,
                padding: SMALL_PADDING,
                outer_width: UiSize::FitChildren.into(),
                outer_height: UiSize::Rest(1.0).into(),
                dt: ui_info.dt
            };

            ui_info.worlds.update(panel, |part| id(WorldSelectPartId::List(part)), list_info, |info, parent, item, is_selected|
            {
                let id = |part| id(WorldSelectPartId::Button(item.id, part));

                let body = parent.update(id(WorldButtonPartId::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(if is_selected { ACCENT_COLOR } else { BACKGROUND_COLOR })),
                    height: item_height.into(),
                    animation: Animation{
                        mix: Some(MixAnimation::default()),
                        ..Default::default()
                    },
                    children_layout: UiLayout::Horizontal,
                    ..Default::default()
                });

                add_padding_horizontal(body, 0.005.into());
                body.update(id(WorldButtonPartId::Text), UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(font_size, item.name.clone())),
                    mix: Some(MixColorLch::color(if is_selected { BACKGROUND_COLOR } else { ACCENT_COLOR })),
                    animation: Animation{
                        mix: Some(MixAnimation::default()),
                        ..Default::default()
                    },
                    ..UiElement::fit_content()
                });
                add_padding_horizontal(body, 0.005.into());

                if is_selected && info.controls.take_click_down()
                {
                    ui_info.info.name = TextboxInfo::new(item.name.clone());
                    action = MenuAction::Start;
                }
            });

            add_padding_vertical(outer_panel, UiSize::Rest(1.0).into());
            let buttons = outer_panel.update(id(WorldSelectPartId::Buttons), UiElement{
                ..Default::default()
            });

            let create_info = {
                let padding: UiElementSize<_> = 0.005.into();

                ButtonInfo{
                    name: "create".to_owned(),
                    width: UiSize::FitChildren.into(),
                    height: item_height.into(),
                    body_width: UiSize::FitChildren.into(),
                    override_font_size: Some(font_size),
                    padding_left: padding.clone(),
                    padding_right: padding,
                    align_left: false,
                    disabled: false,
                    invert_colors: true
                }
            };

            let back_clicked = Self::update_button(
                ui_info.controls,
                buttons,
                |part| id(WorldSelectPartId::Back(part)),
                ButtonInfo{
                    name: "back".to_owned(),
                    ..create_info.clone()
                }
            );

            if back_clicked
            {
                state = MenuState::Main;
            }

            add_padding_horizontal(buttons, 0.005.into());

            let create_clicked = Self::update_button(
                ui_info.controls,
                buttons,
                |part| id(WorldSelectPartId::Create(part)),
                create_info
            );

            if create_clicked
            {
                state = MenuState::WorldCreate;
            }

            add_padding_vertical(outer_panel, UiSize::Pixels(SMALL_PADDING).into());
            add_padding_vertical(menu, 0.05.into());
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
            MenuState::Main | MenuState::Options => true,
            MenuState::Controls | MenuState::WorldSelect | MenuState::WorldCreate => false,
        };

        self.controller.as_inserter().element().children_layout = if align_left
        {
            add_padding_horizontal(self.controller.as_inserter(), 0.08.into());

            UiLayout::Horizontal
        } else
        {
            UiLayout::Vertical
        };

        let menu = {
            let (width, children_layout) = match self.state
            {
                MenuState::Options => (UiSize::Rest(1.0).into(), UiLayout::Horizontal),
                _ => (UiSize::FitChildren.into(), UiLayout::Vertical)
            };

            self.controller.update(MainMenuId::Menu, UiElement{
                width,
                height: UiSize::Rest(1.0).into(),
                children_layout,
                ..Default::default()
            })
        };

        let (next_state, action) = {
            let ui_info = UiInfo{
                controls: &mut controls,
                bindings: &mut self.bindings,
                sliced_textures: &self.sliced_textures,
                fonts: &self.fonts,
                info: &mut self.info,
                worlds: &mut self.worlds,
                controls_taken: &mut self.controls_taken,
                state: self.state,
                dt
            };

            match self.state
            {
                MenuState::Main => Self::update_main(ui_info, menu),
                MenuState::Options => Self::update_options(ui_info, menu),
                MenuState::Controls => Self::update_controls(ui_info, menu),
                MenuState::WorldSelect => Self::update_world_select(ui_info, menu),
                MenuState::WorldCreate => Self::update_world_create(ui_info, menu)
            }
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
