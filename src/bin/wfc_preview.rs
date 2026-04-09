#![allow(clippy::suspicious_else_formatting)]

use std::{
    f32,
    fs,
    ops::Range,
    collections::HashMap,
    cell::RefCell,
    rc::Rc,
    path::PathBuf
};

use fastrand::Rng;

use vulkano::pipeline::graphics::{
    rasterization::CullMode,
    depth_stencil::{
        DepthState,
        CompareOp
    }
};

use nalgebra::{vector, Vector2, Vector3, Matrix4};

use yanyaengine::{
    game_object::*,
    TransformContainer,
    FontsContainer,
    ShadersContainer,
    ShaderId,
    Shader,
    ShadersGroup,
    Object,
    Control,
    App,
    YanyaApp,
    camera::Camera
};

use stephanie::{
    extra_common::*,
    server::world::{
        world_generator::{
            WORLD_CHUNK_SIZE,
            WorldChunkId,
            Entropies,
            WaveCollapser,
            WorldPlane,
            ChunkRules,
            ChunkRulesGroup,
            ChunkGenerator,
            ConditionalInfo
        }
    },
    client::{
        ui_common::*,
        tiles_factory::{TilesFactory, ChunkModelBuilder},
        game_state::{
            UiControls,
            ControlsController,
            Control as GameControl,
            default_bindings
        }
    },
    common::{
        with_z,
        with_error,
        some_or_return,
        some_or_unexpected_return,
        SeededRandom,
        render_info::*,
        lisp::*,
        Pos3,
        FlatChunksContainer,
        TileMap,
        tilemap::TileLoot,
        colors::Lcha,
        world::{
            CHUNK_SIZE,
            TILE_SIZE,
            CHUNK_VISUAL_SIZE,
            TileRotation,
            MaybeGroup,
            DirectionsGroup,
            LocalPos
        }
    }
};


mod ui_vertex
{
    vulkano_shaders::shader!
    {
        ty: "vertex",
        path: "shaders/ui.vert"
    }
}

mod ui_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/ui.frag"
    }
}

mod ui_fill_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/ui_fill.frag"
    }
}

mod textured_fragment
{
    vulkano_shaders::shader!
    {
        ty: "fragment",
        path: "shaders/textured.frag"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiId
{
    Screen,
    ScreenBody,
    Scrollbar(UiScrollbarId, UiScrollbarPart),
    TextboxLabel(TextboxId),
    Textbox(TextboxId, TextboxPartId),
    Button(ButtonId, ButtonPartId),
    ChunkHighlight,
    ChunkInfo,
    MinHighlight(u32),
    SeedTextPanel,
    SeedText,
    StepPanel,
    StepText,
    StatesPanel,
    StatesPanelY(u32),
    State(u32, StatePart),
    Padding(u32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum StatePart
{
    ItemPanel,
    Panel,
    Text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ButtonId
{
    ToggleStep,
    StepOnce,
    StepTen,
    Clear,
    Regenerate
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ButtonPartId
{
    Panel,
    Body,
    Text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TextboxId
{
    Seed
}

impl TextboxId
{
    fn name(&self) -> String
    {
        match self
        {
            Self::Seed => "seed".to_owned()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiScrollbarId
{
    Size
}

const SIZE_RANGE: Range<usize> = 1..15;

impl UiScrollbarId
{
    #[allow(clippy::wrong_self_convention)]
    fn from_f32(&self, tags: &mut Tags, value: f32)
    {
        match self
        {
            Self::Size =>
            {
                let span = SIZE_RANGE.end - SIZE_RANGE.start;
                tags.world_size = (((value * span as f32).floor() as usize) + SIZE_RANGE.start) * 2 + 1;
            }
        }
    }

    #[allow(clippy::wrong_self_convention)]
    fn to_f32(&self, tags: &Tags) -> f32
    {
        match self
        {
            Self::Size =>
            {
                let span = SIZE_RANGE.end - SIZE_RANGE.start;
                ((tags.world_size - 1) / 2 - SIZE_RANGE.start) as f32 / span as f32
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiScrollbarPart
{
    Panel,
    Text,
    Body,
    Bar
}

impl Idable for UiId
{
    fn screen() -> Self { Self::Screen }

    fn padding(id: u32) -> Self { Self::Padding(id) }
}

struct ChunkPreview
{
    tiles: Vec<Object>
}

#[derive(Debug, Clone, PartialEq)]
struct Tags
{
    step_by_step: bool,
    world_size: usize,
    seed_text: TextboxWrapper,
    seed: u64
}

fn parse_rules() -> Option<Rc<ChunkRulesGroup>>
{
    let rules = ChunkRulesGroup::load(PathBuf::from("world_generation/"));

    if let Err(err) = &rules
    {
        eprintln!("error creating chunk_rules: {err}");
    }

    rules.ok().map(|rules| Rc::new(rules))
}

struct AssetsDependent
{
    tilemap: Option<(LispMemory, TilesFactory)>,
    rules: Option<Rc<ChunkRulesGroup>>,
    world_chunks: Rc<RefCell<WorldPlane>>
}

impl AssetsDependent
{
    fn new(
        info: &mut ObjectCreatePartialInfo,
        rules: Option<Rc<ChunkRulesGroup>>,
        world_chunks: Rc<RefCell<WorldPlane>>
    ) -> Self
    {
        let tilemap = {
            let tilemap = with_error(TileMap::parse(
                TileLoot{
                    client: &mut Vec::new()
                },
                "info/tiles.json",
                "textures/tiles/"
            ));

            tilemap.zip(rules.as_ref().map(|x| x.clone())).and_then(|(tilemap, rules)|
            {
                let overmaps = Rc::new(RefCell::new(vec![world_chunks.clone()]));

                let primitives = Rc::new(ChunkGenerator::default_primitives(&tilemap.tilemap, rules, overmaps, true));

                let memory = LispMemory::new(primitives, 256, 1 << 13);

                let tiles_factory = with_error(TilesFactory::new(info, tilemap))?;

                Some((memory, tiles_factory))
            })
        };

        Self{
            tilemap,
            rules,
            world_chunks
        }
    }

    fn reload(&mut self, info: &mut UpdateBuffersInfo)
    {
        let assets = info.partial.assets.clone();
        assets.lock().reload(info);

        *self = Self::new(&mut info.partial, parse_rules(), self.world_chunks.clone());
    }
}

const PANEL_WIDTH: f32 = 70.0;
const PANEL_HEIGHT: f32 = 70.0;
const TEXT_HEIGHT: f32 = 10.0;

struct StatesTooltip
{
    chunk_positions: Option<Vec<Vec<Object>>>,
    states: Vec<WorldChunkId>,
    mouse_position: Vector2<f32>
}

struct ChunkPreviewer
{
    shaders: DrawShaders,
    fonts: Rc<FontsContainer>,
    world_chunks: Rc<RefCell<WorldPlane>>,
    assets_dependent: ModifiedWatcher<AssetsDependent>,
    controls: ControlsController<UiId>,
    camera_position: Vector2<f32>,
    camera_zoom: f32,
    camera: Camera,
    ui_camera: Camera,
    controller: Controller<UiId>,
    update_timer: f32,
    regenerate: bool,
    selected_textbox: Option<UiId>,
    chunk_code: HashMap<String, ModifiedWatcher<Lisp>>,
    seed_rng: Rng,
    current_generator: Option<(Entropies, SeededRandom)>,
    states_tooltip: Option<StatesTooltip>,
    current_tags: ModifiedWatcher<Tags>,
    preview_tags: ModifiedWatcher<Tags>,
    preview: Option<ChunkPreview>
}

const PARENT_DIRECTORY: &str = "world_generation";

impl ChunkPreviewer
{
    fn compile_chunk(&mut self, name: String)
    {
        let memory = some_or_return!(self.assets_dependent.tilemap.as_ref()).0.clone();

        let parent_directory = PathBuf::from(PARENT_DIRECTORY);
        let chunks_directory = parent_directory.join("chunks");
        let filepath = chunks_directory.join(format!("{name}.scm"));

        if !filepath.exists()
        {
            self.chunk_code.remove(&name);
            return;
        }

        if !self.chunk_code.get_mut(&name).map(|x| x.modified_check()).unwrap_or(true)
        {
            return;
        }

        let standard_path = "lisp/standard.scm";
        let standard_code = fs::read_to_string(standard_path).unwrap_or_else(|err|
        {
            panic!("cant load {standard_path}: {err}")
        });

        let default_path = parent_directory.join("default.scm");
        let default_code = fs::read_to_string(&default_path).unwrap_or_else(|err|
        {
            panic!("cant load {}: {err}", default_path.display())
        });

        let chunk_code = fs::read_to_string(&filepath).unwrap_or_else(|err|
        {
            panic!("cant load {}: {err}", filepath.display())
        });

        let depend_paths: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(vec![
            standard_path.into(),
            default_path.into(),
            filepath.into()
        ]));

        let config = LispConfig{
            load_handler: {
                let depend_paths = depend_paths.clone();
                let parent_directory = chunks_directory;
                Some(Box::new(move |filename|
                {
                    let load_path = parent_directory.join(filename);

                    depend_paths.borrow_mut().push(load_path.clone());

                    match fs::read_to_string(load_path)
                    {
                        Ok(x) => Some(x),
                        Err(err) =>
                        {
                            eprintln!("error trying to load `{filename}`: {err}");

                            None
                        }
                    }
                }))
            },
            memory,
            ..Default::default()
        };

        match Lisp::new_with_config(config, &[&standard_code, &default_code, &chunk_code])
        {
            Ok(lisp) =>
            {
                if let Some(chunk_code) = self.chunk_code.get_mut(&name)
                {
                    **chunk_code = lisp;
                } else
                {
                    self.chunk_code.insert(name, ModifiedWatcher::new_many(depend_paths.borrow().clone(), lisp));
                }
            },
            Err(err) => eprintln!("error compiling {name}: {err}")
        }
    }
}

struct DrawShaders
{
    normal: ShaderId,
    ui: ShaderId,
    ui_fill: ShaderId
}

impl YanyaApp for ChunkPreviewer
{
    type SetupInfo = ();
    type AppInfo = Option<DrawShaders>;

    fn init(mut info: InitPartialInfo<Self::SetupInfo>, app_info: Self::AppInfo) -> Self
    {
        let controls = ControlsController::new(default_bindings());

        let camera_position = Vector2::new(-0.7, 0.0);
        let camera_zoom = 3.0 * WORLD_CHUNK_SIZE.x as f32;
        let mut camera = Camera::new(info.object_info.aspect(), -1.0..1.0);
        camera.rescale(camera_zoom);
        camera.set_position(with_z(camera_position, 0.0).into());

        let ui_camera = Camera::new(info.object_info.aspect(), -1.0..1.0);

        let controller = Controller::new(&info.object_info);

        let mut seed_rng = Rng::new();
        let tags = ModifiedWatcher::new(PARENT_DIRECTORY, Tags{
            step_by_step: true,
            world_size: 15,
            seed_text: String::new().into(),
            seed: seed_rng.u64(..)
        });

        let preview = None;

        let world_chunks = Rc::new(RefCell::new(WorldPlane(FlatChunksContainer::new(Pos3::new(tags.world_size, tags.world_size, 1)))));

        let assets_dependent = ModifiedWatcher::new_many(
            vec!["textures".into(), "info".into(), "lisp".into()],
            AssetsDependent::new(&mut info.object_info, parse_rules(), world_chunks.clone())
        );

        Self{
            shaders: app_info.unwrap(),
            fonts: info.object_info.builder_wrapper.fonts().clone(),
            world_chunks,
            assets_dependent,
            controls,
            camera_position,
            camera_zoom,
            camera,
            ui_camera,
            controller,
            update_timer: 0.0,
            regenerate: false,
            selected_textbox: None,
            chunk_code: HashMap::new(),
            seed_rng,
            current_generator: None,
            states_tooltip: None,
            current_tags: tags.clone(),
            preview_tags: tags,
            preview
        }
    }

    fn update(&mut self, partial_info: UpdateBuffersPartialInfo, dt: f32)
    {
        let mut info = partial_info.to_full(&self.ui_camera);

        if self.update_timer <= 0.0
        {
            if self.assets_dependent.modified_check()
            {
                eprintln!("hot reloading assets");
                self.assets_dependent.reload(&mut info);

                self.regenerate = true;
            }
        }

        let mut controls = self.controls.changed_this_frame();

        {
            let mut all_exist = true;
            let mut these_positions = Vec::new();

            let logical_position;

            let states_at = |this: &Self, logical_position: Vector2<i32>| -> Option<_>
            {
                let (entropies, _) = this.current_generator.as_ref()?;

                let size = {
                    let size = this.preview_tags.world_size;

                    Pos3::new(size, size, 1)
                };

                let local_pos = LocalPos::new(Pos3::new(logical_position.x as usize, logical_position.y as usize, 0), size);

                local_pos.in_bounds().then(|| entropies.get(local_pos).clone())
            };

            let absolute_camera = self.camera_position / self.camera_zoom;

            let tile_size = TILE_SIZE / self.camera_zoom;
            let chunk_size = Vector3::from(WORLD_CHUNK_SIZE).xy().map(|x| x as f32 * tile_size);

            let world_size = self.current_tags.world_size;
            let half_offset = vector![(world_size / 2) as i32, (world_size / 2) as i32];

            let tile_position_of = |tiled_position: Vector2<i32>| -> Vector2<f32>
            {
                (tiled_position.map(|x| x as f32).component_mul(&chunk_size) - absolute_camera)
                    - (chunk_size * 0.5)
                    - (half_offset.map(|x| x as f32).component_mul(&chunk_size))
            };

            let controls = &mut controls;

            let aspect = self.ui_camera.aspect();

            let screen_body = self.controller.update(UiId::ScreenBody, UiElement{
                children_layout: UiLayout::Vertical,
                width: aspect.min(1.0).into(),
                height: aspect.recip().min(1.0).into(),
                ..Default::default()
            });

            {
                let tiled_position = {
                    let tile = (self.controller.mouse_position() + absolute_camera).map(|x| (x / tile_size).floor() as i32);

                    tile.zip_map(&(Vector3::from(WORLD_CHUNK_SIZE.map(|x| x as i32))).xy(), |a, b|
                    {
                        if a < 0
                        {
                            (a + 1) / b
                        } else
                        {
                            (a / b) + 1
                        }
                    }) + half_offset
                };

                let tile_position = tile_position_of(tiled_position);

                self.controller.update(UiId::ChunkHighlight, UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{a: 0.5, ..WHITE_COLOR})),
                    width: UiSize::Absolute(chunk_size.x).into(),
                    height: UiSize::Absolute(chunk_size.y).into(),
                    position: UiPosition::Absolute{position: tile_position, align: UiPositionAlign::default()},
                    ..Default::default()
                });

                logical_position = tiled_position.map(|x| x as i32);

                let tile_info_text = format!(
                    "{}, {} (entropy: {})",
                    logical_position.x, logical_position.y,
                    states_at(self, logical_position).map(|states| format!("{:.2}", states.entropy())).unwrap_or_else(|| "?".to_owned())
                );

                self.controller.update(UiId::ChunkInfo, UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(12, tile_info_text)),
                    position: UiPosition::Absolute{position: tile_position + vector![0.0, -tile_size], align: UiPositionAlign::default()},
                    ..UiElement::fit_content()
                });
            }

            if let Some(StatesTooltip{states, mouse_position, ..}) = self.states_tooltip.as_ref()
            {
                let panel = self.controller.update(UiId::StatesPanel, UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.5})),
                    position: UiPosition::Absolute{position: *mouse_position, align: UiPositionAlign::default()},
                    children_layout: UiLayout::Vertical,
                    ..Default::default()
                });

                let padding = 5.0;
                let per_row = 10;

                (||
                {
                    for y in 0..
                    {
                        add_padding_vertical(panel, UiSize::Pixels(padding).into());

                        let panel_y = panel.update(UiId::StatesPanelY(y as u32), UiElement{
                            children_layout: UiLayout::Horizontal,
                            ..Default::default()
                        });

                        for x in 0..per_row
                        {
                            let index = x + y * per_row;

                            if states.len() == index
                            {
                                add_padding_horizontal(panel_y, UiSize::Pixels(padding).into());

                                return;
                            }

                            add_padding_horizontal(panel_y, UiSize::Pixels(padding).into());

                            let this_state = states[index];

                            let state_name = self.assets_dependent.rules.as_ref().map(|rules|
                            {
                                let info = rules.surface.get(this_state);

                                let direction = match info.rotation()
                                {
                                    TileRotation::Up => "",
                                    TileRotation::Right => "> ",
                                    TileRotation::Left => "< ",
                                    TileRotation::Down => "V "
                                };

                                format!("{}{}", direction, info.name())
                            }).unwrap_or("idk".to_owned());

                            let item_panel = panel_y.update(UiId::State(index as u32, StatePart::ItemPanel), UiElement{
                                children_layout: UiLayout::Vertical,
                                ..Default::default()
                            });

                            item_panel.update(UiId::State(index as u32, StatePart::Text), UiElement{
                                texture: UiTexture::Text(TextInfo::new_simple(8, state_name)),
                                mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 1.0})),
                                height: UiSize::Pixels(TEXT_HEIGHT).into(),
                                ..UiElement::fit_content()
                            });

                            let panel = item_panel.update(UiId::State(index as u32, StatePart::Panel), UiElement{
                                texture: UiTexture::Solid,
                                mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.0})),
                                width: UiSize::Pixels(PANEL_WIDTH).into(),
                                height: UiSize::Pixels(PANEL_HEIGHT - TEXT_HEIGHT).into(),
                                ..Default::default()
                            });

                            if let Some(this_position) = panel.try_position()
                            {
                                these_positions.push((this_position, this_state));
                            } else
                            {
                                all_exist = false;
                            }
                        }

                        add_padding_horizontal(panel_y, UiSize::Pixels(padding).into());
                    }
                })();

                add_padding_vertical(panel, UiSize::Pixels(padding).into());
            }

            let mut update_scrollbar = |this_id, tags: &mut Tags|
            {
                let id = |part_id|
                {
                    UiId::Scrollbar(this_id, part_id)
                };

                let panel = screen_body.update(id(UiScrollbarPart::Panel), UiElement{
                    width: UiElementSize{
                        minimum_size: Some(UiMinimumSize::FitChildren),
                        size: UiSize::Rest(1.0)
                    },
                    children_layout: UiLayout::Horizontal,
                    ..Default::default()
                });

                let scrollbar_id = id(UiScrollbarPart::Body);
                let body = panel.update(scrollbar_id, UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.5})),
                    width: UiSize::Pixels(250.0).into(),
                    height: UiSize::Pixels(30.0).into(),
                    children_layout: UiLayout::Horizontal,
                    ..Default::default()
                });

                let bar_width = 0.1;

                let is_horizontal = true;
                if let Some(value) = scrollbar_handle(
                    controls,
                    body,
                    &scrollbar_id,
                    bar_width,
                    is_horizontal,
                    false
                )
                {
                    this_id.from_f32(tags, value);
                }

                let scroll = this_id.to_f32(tags);
                add_padding_horizontal(body, UiSize::Rest(scroll).into());
                let bar = body.update(id(UiScrollbarPart::Bar), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.5})),
                    width: UiSize::CopyElement(UiDirection::Horizontal, bar_width, scrollbar_id).into(),
                    height: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });
                add_padding_horizontal(body, UiSize::Rest(1.0 - scroll).into());

                if bar.is_mouse_inside() || controls.observe_action_held(&scrollbar_id)
                {
                    bar.element().mix = Some(MixColorLch::color(Lcha{l: 40.0, c: 0.0, h: 0.0, a: 0.5}));
                }

                let description = match this_id
                {
                    UiScrollbarId::Size =>
                    {
                        format!("size: {}", tags.world_size)
                    }
                };

                add_padding_horizontal(panel, UiSize::Pixels(30.0).into());
                panel.update(id(UiScrollbarPart::Text), UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(20, description)),
                    ..UiElement::fit_content()
                });
            };

            update_scrollbar(UiScrollbarId::Size, &mut self.current_tags);

            let font_size = 12;

            let update_button = |controls: &mut UiControls<_>, name: &str, id|
            {
                add_padding_vertical(screen_body, UiSize::Pixels(10.0).into());

                let panel = screen_body.update(UiId::Button(id, ButtonPartId::Panel), UiElement{
                    width: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });

                let button = panel.update(UiId::Button(id, ButtonPartId::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.5})),
                    width: UiSize::Pixels(150.0).into(),
                    children_layout: UiLayout::Vertical,
                    ..Default::default()
                });

                button.update(UiId::Button(id, ButtonPartId::Text), UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(font_size, name.to_owned())),
                    ..UiElement::fit_content()
                });

                button.is_mouse_inside() && controls.take_click_down()
            };

            let mut update_textbox = |controls: &mut UiControls<_>, textbox_id: TextboxId, text: &mut TextboxInfo, centered|
            {
                let id = |part| UiId::Textbox(textbox_id, part);

                let parent = if centered
                {
                    screen_body
                } else
                {
                    screen_body.update(id(TextboxPartId::Panel), UiElement{
                        width: UiSize::Rest(1.0).into(),
                        ..Default::default()
                    })
                };

                if !centered
                {
                    parent.update(UiId::TextboxLabel(textbox_id), UiElement{
                        texture: UiTexture::Text(TextInfo::new_simple(font_size, textbox_id.name())),
                        ..UiElement::fit_content()
                    });

                    add_padding_horizontal(parent, UiSize::Pixels(10.0).into());
                }

                let name_body = parent.update(id(TextboxPartId::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.3})),
                    width: UiElementSize{minimum_size: Some(UiMinimumSize::Pixels(250.0)), size: UiSize::FitChildren},
                    height: UiSize::Pixels(20.0).into(),
                    ..Default::default()
                });

                if name_body.is_mouse_inside() && controls.take_click_down()
                {
                    self.selected_textbox = Some(id(TextboxPartId::Body));
                }

                add_padding_horizontal(name_body, UiSize::Pixels(10.0).into());

                if self.selected_textbox.as_ref().map(|x| *x == id(TextboxPartId::Body)).unwrap_or(false)
                {
                    name_body.element().mix = Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.5}));

                    textbox_update(controls, &self.fonts, id, name_body, font_size, text);
                } else
                {
                    name_body.update(id(TextboxPartId::Text), UiElement{
                        texture: UiTexture::Text(TextInfo::new_simple(font_size, text.text.clone())),
                        ..UiElement::fit_content()
                    });
                }

                add_padding_horizontal(name_body, UiSize::Pixels(10.0).into());
            };

            add_padding_vertical(screen_body, UiSize::Pixels(15.0).into());

            {
                update_textbox(controls, TextboxId::Seed, &mut self.current_tags.seed_text.0, false);

                let text = &self.current_tags.seed_text.0.text;
                if !text.is_empty()
                {
                    match text.parse::<u64>()
                    {
                        Ok(seed) => self.current_tags.seed = seed,
                        Err(err) => eprintln!("error parsing seed ({text}): {err}")
                    }
                }
            }

            add_padding_vertical(screen_body, UiSize::Pixels(10.0).into());

            {
                let seed_text = format!("seed: {}", self.current_tags.seed);

                let panel = screen_body.update(UiId::SeedTextPanel, UiElement{
                    children_layout: UiLayout::Horizontal,
                    width: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });

                panel.update(UiId::SeedText, UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(font_size, seed_text)),
                    ..UiElement::fit_content()
                });
            }

            if update_button(controls, "toggle step", ButtonId::ToggleStep)
            {
                self.current_tags.step_by_step = !self.current_tags.step_by_step;
            }

            add_padding_vertical(screen_body, UiSize::Pixels(10.0).into());

            {
                let step_text = format!("step by step: {}", if self.current_tags.step_by_step { "on" } else { "off" });

                let panel = screen_body.update(UiId::StepPanel, UiElement{
                    children_layout: UiLayout::Horizontal,
                    width: UiSize::Rest(1.0).into(),
                    ..Default::default()
                });

                panel.update(UiId::StepText, UiElement{
                    texture: UiTexture::Text(TextInfo::new_simple(font_size, step_text)),
                    ..UiElement::fit_content()
                });
            }

            let needs_step_once = update_button(controls, "step once", ButtonId::StepOnce);
            let needs_step_ten = update_button(controls, "step ten", ButtonId::StepTen);

            if update_button(controls, "clear", ButtonId::Clear)
            {
                self.regenerate = true;
            }

            if update_button(controls, "regenerate", ButtonId::Regenerate)
            {
                self.current_tags.seed = self.seed_rng.u64(..);
                eprintln!("new seed: {}", self.current_tags.seed);

                self.regenerate = true;
            }

            if needs_step_once
            {
                self.do_step_n(1);
            }

            if needs_step_ten
            {
                self.do_step_n(10);
            }

            if let Some((entropies, _)) = self.current_generator.as_ref()
            {
                let mut lowest_entropy = f64::MAX;
                let mut mins: Vec<LocalPos> = Vec::new();

                for pos in entropies.positions()
                {
                    let value = entropies.get(pos);

                    if !value.collapsed()
                    {
                        let entropy = value.entropy();

                        if entropy < lowest_entropy
                        {
                            lowest_entropy = entropy;

                            mins.clear();
                            mins.push(pos);
                        } else if entropy == lowest_entropy
                        {
                            mins.push(pos);
                        }
                    }
                }

                mins.iter().enumerate().for_each(|(index, x)|
                {
                    self.controller.update(UiId::MinHighlight(index as u32), UiElement{
                        texture: UiTexture::Solid,
                        mix: Some(MixColorLch::color(Lcha{l: 80.0, c: 100.0, h: 0.0, a: 0.5})),
                        width: UiSize::Absolute(chunk_size.x).into(),
                        height: UiSize::Absolute(chunk_size.y).into(),
                        position: UiPosition::Absolute{
                            position: tile_position_of(Vector3::from(x.pos).xy().cast()),
                            align: UiPositionAlign::default()
                        },
                        ..Default::default()
                    });
                });
            }

            {
                let new_positions = all_exist.then(|| -> Vec<Vec<Object>>
                {
                    these_positions.into_iter().map(|(this_position, this_state)|
                    {
                        let screen_size = Vector2::from(info.partial.size);
                        let aspect_size = screen_size / screen_size.max();

                        let mut objects = self.chunk_objects(None, this_state);
                        objects.iter_mut().for_each(|object|
                        {
                            let scale = vector![PANEL_WIDTH - TEXT_HEIGHT, PANEL_HEIGHT - TEXT_HEIGHT].component_div(&screen_size);
                            let offset = vector![PANEL_WIDTH - TEXT_HEIGHT * 2.0, PANEL_HEIGHT - TEXT_HEIGHT * 2.0].component_div(&screen_size);

                            object.translate(with_z(this_position.component_div(&aspect_size) * 2.0 - offset, 0.0));

                            object.set_scale(with_z(scale * 2.0, 1.0));
                        });

                        objects
                    }).collect()
                });

                if let Some(StatesTooltip{chunk_positions, ..}) = self.states_tooltip.as_mut()
                {
                    *chunk_positions = new_positions;
                }
            }

            if controls.is_click_down() && !controls.is_click_taken()
            {
                if let Some(states) = states_at(self, logical_position)
                {
                    let states = states.states().to_vec();

                    self.states_tooltip = Some(StatesTooltip{chunk_positions: None, states, mouse_position: self.controller.mouse_position()});
                } else
                {
                    self.states_tooltip = None;
                }
            }
        }

        self.controller.create_renders(&mut info, dt);
        self.controller.update_buffers(&mut info);

        self.camera.rescale(self.camera_zoom);
        self.camera.set_position(with_z(self.camera_position, 0.0).into());
        self.camera.update();

        info.update_camera(&self.camera);

        let needs_recreate = self.preview.is_none() || self.current_tags != self.preview_tags;
        let recreate_preview = self.regenerate || (self.update_timer <= 0.0 && needs_recreate);

        if recreate_preview
        {
            self.regenerate = false;

            self.preview_tags = self.current_tags.clone();

            let tags = self.preview_tags.clone();

            self.world_chunks.borrow_mut().0 = FlatChunksContainer::new(Pos3::new(tags.world_size, tags.world_size, 1));

            self.current_generator = {
                let rules = some_or_return!(self.assets_dependent.rules.as_ref());

                let plane = &mut self.world_chunks.borrow_mut().0;

                let wave_collapser = WaveCollapser::new(&rules.surface, plane);

                let entropies = wave_collapser.entropies().clone();

                Some((entropies, SeededRandom::from(self.preview_tags.seed)))
            };

            if !self.preview_tags.step_by_step
            {
                self.with_wave_collapser(|rules, wave_collapser, rng|
                {
                    while let Some((pos, states)) = wave_collapser.lowest_entropy_with(rng)
                    {
                        let generated_chunk = rules.generate(states.collapse(rules, rng));

                        wave_collapser.generate_single(pos, generated_chunk);
                    }
                });
            }

            self.redraw_map();

            self.update_timer = 0.5;
        }

        self.update_timer -= dt;

        if let Some(preview) = self.preview.as_mut()
        {
            preview.tiles.iter_mut().for_each(|x| x.update_buffers(&mut info));
        }

        info.with_projection(Matrix4::identity(), |info|
        {
            if let Some(StatesTooltip{chunk_positions: Some(chunk_positions), ..}) = self.states_tooltip.as_mut()
            {
                chunk_positions.iter_mut().for_each(|x| x.iter_mut().for_each(|x| x.update_buffers(info)));
            }
        });

        if controls.is_click_down() && !controls.is_click_taken()
        {
            self.selected_textbox = None;
        }

        self.controls.consume_changed(controls).for_each(drop);

        let speed = 0.5 * WORLD_CHUNK_SIZE.x as f32 * dt;
        let zoom_speed = 1.2 * WORLD_CHUNK_SIZE.x as f32 * dt;

        if self.controls.is_down(GameControl::MoveRight)
        {
            self.camera_position.x += speed;
        }

        if self.controls.is_down(GameControl::MoveLeft)
        {
            self.camera_position.x -= speed;
        }

        if self.controls.is_down(GameControl::MoveDown)
        {
            self.camera_position.y += speed;
        }

        if self.controls.is_down(GameControl::MoveUp)
        {
            self.camera_position.y -= speed;
        }

        if self.controls.is_down(GameControl::ZoomOut)
        {
            self.camera_zoom += zoom_speed;
        }

        if self.controls.is_down(GameControl::ZoomIn)
        {
            self.camera_zoom = (self.camera_zoom - zoom_speed).max(0.01);
        }
    }

    fn input(&mut self, control: Control)
    {
        self.controls.handle_input(control);
    }

    fn mouse_move(&mut self, (x, y): (f64, f64))
    {
        let normalized_size = self.camera.normalized_size();
        let position = Vector2::new(x as f32, y as f32).component_mul(&normalized_size) - normalized_size * 0.5;
        self.controller.set_mouse_position(position);
    }

    fn draw(&mut self, mut info: DrawInfo)
    {
        if let Some(preview) = self.preview.as_ref()
        {
            info.bind_pipeline(self.shaders.normal);

            preview.tiles.iter().for_each(|x| x.draw(&mut info));
        }

        info.bind_pipeline(self.shaders.ui);

        self.controller.draw(&mut info, &UiShaders{ui: self.shaders.ui, ui_fill: self.shaders.ui_fill});

        info.bind_pipeline(self.shaders.normal);

        if let Some(StatesTooltip{chunk_positions: Some(chunk_positions), ..}) = self.states_tooltip.as_ref()
        {
            chunk_positions.iter().for_each(|x| x.iter().for_each(|x| x.draw(&mut info)));
        }
    }

    fn resize(&mut self, aspect: f32)
    {
        self.ui_camera.resize(aspect);
        self.camera.resize(aspect);
    }
}

impl ChunkPreviewer
{
    fn with_wave_collapser(&mut self, f: impl FnOnce(&ChunkRules, &mut WaveCollapser, &mut SeededRandom))
    {
        self.states_tooltip = None;

        let rules = some_or_return!(self.assets_dependent.rules.as_ref());

        let plane = &mut self.world_chunks.borrow_mut().0;

        let (entropies, mut rng) = some_or_return!(self.current_generator.take());

        let mut wave_collapser = WaveCollapser::new_raw(SeededRandom::from(0), &rules.surface, entropies, plane);

        f(&rules.surface, &mut wave_collapser, &mut rng);

        self.current_generator = Some((wave_collapser.entropies().clone(), rng));
    }

    fn redraw_map(&mut self)
    {
        self.preview.take();

        let size = self.world_chunks.borrow().0.size();

        (0..size.y).for_each(|y|
        {
            (0..size.x).for_each(|x|
            {
                let chunk_pos = LocalPos::new(Pos3::new(x, y, 0), size);

                if self.world_chunks.borrow().0[chunk_pos].is_some()
                {
                    self.generate_chunk_at(chunk_pos);
                }
            });
        });
    }

    fn do_step_n(&mut self, n: usize)
    {
        self.with_wave_collapser(|rules, wave_collapser, rng|
        {
            (0..n).for_each(|_|
            {
                if let Some((pos, states)) = wave_collapser.lowest_entropy_with(rng)
                {
                    let generated_chunk = rules.generate(states.collapse(rules, rng));

                    wave_collapser.generate_single(pos, generated_chunk);
                }
            });
        });

        self.redraw_map();
    }

    fn chunk_objects(
        &mut self,
        chunk_pos: Option<LocalPos>,
        world_chunk_id: WorldChunkId
    ) -> Vec<Object>
    {
        let size = self.preview_tags.world_size;

        let chunk_info = ConditionalInfo{
            position: chunk_pos.unwrap_or_else(|| LocalPos::new(Pos3::repeat(0), Pos3::new(size, size, 1))),
            height: 0,
            difficulty: 0.0
        };

        let chunk_name = {
            let rules = some_or_return!(self.assets_dependent.rules.as_ref());

            some_or_unexpected_return!(rules.name_mappings().world_chunk.get_back(&world_chunk_id).clone()).1.clone()
        };

        let chunk_code = {
            self.compile_chunk(chunk_name.clone());

            some_or_return!(self.chunk_code.get_mut(&chunk_name))
        };

        let rules = some_or_return!(self.assets_dependent.rules.as_ref());

        let tiles = ChunkGenerator::generate_chunk_with(
            &chunk_info,
            &chunk_name,
            rules.rotation(world_chunk_id),
            0,
            chunk_code,
            &mut |_marker| {}
        );

        let tiles_factory = &some_or_return!(self.assets_dependent.tilemap.as_ref()).1;

        let mut chunk_objects = Vec::new();

        match tiles
        {
            Ok(x) =>
            {
                let mut chunk_builder = ChunkModelBuilder::new();

                x.flat_slice_iter(0).for_each(|(pos, tile)|
                {
                    let pos = pos.pos;

                    if tile.is_none()
                    {
                        return;
                    }

                    chunk_builder.create(
                        &tiles_factory.tilemap(),
                        Pos3::new(pos.x, pos.y, 0).into(),
                        Some(MaybeGroup{
                            this: *tile,
                            other: DirectionsGroup{
                                up: if pos.y == 0 { None } else { x.get(Pos3::new(pos.x, pos.y - 1, 0)).copied() },
                                down: if pos.y == (WORLD_CHUNK_SIZE.y - 1) { None } else { x.get(Pos3::new(pos.x, pos.y + 1, 0)).copied() },
                                left: if pos.x == 0 { None } else { x.get(Pos3::new(pos.x - 1, pos.y, 0)).copied() },
                                right: if pos.x == (WORLD_CHUNK_SIZE.x - 1) { None } else { x.get(Pos3::new(pos.x + 1, pos.y, 0)).copied() }
                            }
                        }),
                        tile.0.unwrap()
                    );
                });

                let slices = chunk_builder.build(Pos3::repeat(0).into());

                if let Some(mut slice_info) = slices.into_iter().next().unwrap()
                {
                    let chunk_ratio = with_z(
                        Vector2::new(CHUNK_SIZE, CHUNK_SIZE).component_div(&Vector3::from(WORLD_CHUNK_SIZE).xy()),
                        1
                    );

                    let half_offset = Pos3::new((size / 2) as i32, (size / 2) as i32, 0);

                    let pos_offset = if let Some(chunk_pos) = chunk_pos
                    {
                        let pos_offset: Vector3<f32> = Vector3::from(chunk_pos.pos.map(|x| x as i32) - half_offset).cast();

                        Vector3::repeat(-CHUNK_VISUAL_SIZE * 0.5) + (pos_offset * CHUNK_VISUAL_SIZE).component_div(&chunk_ratio.cast())
                    } else
                    {
                        Vector3::zeros()
                    };

                    slice_info.transform.position += pos_offset;
                    chunk_objects.push(tiles_factory.build_slice(slice_info));
                }
            },
            Err(err) => eprintln!("{err} in ({chunk_name})")
        }

        chunk_objects
    }

    fn generate_chunk_at(
        &mut self,
        chunk_pos: LocalPos
    )
    {
        let world_chunk_id = self.world_chunks.borrow().0[chunk_pos].as_ref().unwrap().id();

        let chunk_objects = self.chunk_objects(Some(chunk_pos), world_chunk_id);

        if let Some(preview) = self.preview.as_mut()
        {
            preview.tiles.extend(chunk_objects);
        } else
        {
            self.preview = Some(ChunkPreview{tiles: chunk_objects});
        }
    }
}

fn main()
{
    let mut shaders = ShadersContainer::new();

    let normal = shaders.push(Shader{
        shader: ShadersGroup::new(
            ui_vertex::load,
            textured_fragment::load
        ),
        depth: Some(DepthState{
            write_enable: true,
            compare_op: CompareOp::Always
        }),
        per_vertex: Some(vec![Object::per_vertex()]),
        subpass: 0,
        cull: CullMode::None,
        ..Default::default()
    });

    let ui = shaders.push(Shader{
        shader: ShadersGroup::new(
            ui_vertex::load,
            ui_fragment::load
        ),
        depth: Some(DepthState{
            write_enable: true,
            compare_op: CompareOp::Always
        }),
        per_vertex: Some(vec![Object::per_vertex()]),
        subpass: 0,
        cull: CullMode::None,
        ..Default::default()
    });

    let ui_fill = shaders.push(Shader{
        shader: ShadersGroup::new(
            ui_vertex::load,
            ui_fill_fragment::load
        ),
        depth: Some(DepthState{
            write_enable: true,
            compare_op: CompareOp::Always
        }),
        per_vertex: Some(vec![Object::per_vertex()]),
        subpass: 0,
        cull: CullMode::None,
        ..Default::default()
    });

    App::<ChunkPreviewer>::new()
        .with_title("wfc preview")
        .with_textures_path("textures")
        .with_shaders(shaders)
        .with_app_init(Some(DrawShaders{normal, ui, ui_fill}))
        .with_clear_color([0.4, 0.4, 0.45])
        .run();
}

#[cfg(test)]
mod tests
{
    use super::*;


    fn do_test_step(rules: &ChunkRules, wave_collapser: &mut WaveCollapser, rng: &mut SeededRandom)
    {
        if let Some((pos, states)) = wave_collapser.lowest_entropy_with(rng)
        {
            let generated_chunk = rules.generate(states.collapse(rules, rng));

            wave_collapser.generate_single(pos, generated_chunk);
        }
    }

    #[test]
    fn deterministic_wfc()
    {
        let rules = ChunkRulesGroup::load(PathBuf::from("world_generation/")).unwrap();

        let mut plane_one = FlatChunksContainer::new(Pos3::new(5, 5, 1));

        let one = {
            let mut rng_one = SeededRandom::from(5);

            let mut wave_collapser_one = WaveCollapser::new(&rules.surface, &mut plane_one);
            (0..3).for_each(|_| do_test_step(&rules.surface, &mut wave_collapser_one, &mut rng_one));

            wave_collapser_one
        };

        let mut plane_two = FlatChunksContainer::new(Pos3::new(5, 5, 1));

        let two = {
            let mut rng_two = SeededRandom::from(5);

            let mut wave_collapser_two_temp = WaveCollapser::new(&rules.surface, &mut plane_two);
            do_test_step(&rules.surface, &mut wave_collapser_two_temp, &mut rng_two);

            let mut wave_collapser_two = WaveCollapser::new(&rules.surface, &mut plane_two);
            (0..2).for_each(|_| do_test_step(&rules.surface, &mut wave_collapser_two, &mut rng_two));

            wave_collapser_two
        };

        assert_eq!(one.entropies(), two.entropies());
        assert_eq!(plane_one, plane_two);
    }
}
