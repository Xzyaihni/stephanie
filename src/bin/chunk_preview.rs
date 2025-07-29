#![allow(clippy::suspicious_else_formatting)]

use std::{
    f32,
    fs,
    ops::Range,
    time::SystemTime,
    rc::Rc,
    path::PathBuf
};

use vulkano::pipeline::graphics::{
    rasterization::CullMode,
    depth_stencil::{
        DepthState,
        CompareOp
    }
};

use nalgebra::{Vector2, Vector3};

use yanyaengine::{
    game_object::*,
    ShadersContainer,
    Transform,
    ShaderId,
    Shader,
    ShadersGroup,
    Object,
    ObjectInfo,
    DefaultModel,
    Control,
    App,
    YanyaApp,
    camera::Camera
};

use stephanie::{
    server::world::{
        MarkerKind,
        world_generator::{
            WorldChunkTag,
            ChunkRulesGroup,
            ChunkGenerator,
            ConditionalInfo
        }
    },
    client::game_state::{
        ControlsController,
        ui::controller::*
    },
    common::{
        render_info::*,
        lisp::*,
        TileMap,
        colors::Lcha,
        world::{TILE_SIZE, CHUNK_SIZE, Tile, TileRotation}
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
    Textbox(TextboxId, TextboxPartId),
    Button(ButtonId, ButtonPartId),
    Padding(u32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ButtonId
{
    Add,
    Remove,
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
    Name,
    Tag(u32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TextboxPartId
{
    Panel,
    Body,
    Text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UiScrollbarId
{
    Height,
    Difficulty
}

const HEIGHT_RANGE: Range<i32> = -5..20;
const DIFFICULTY_MAX: f32 = 5.0;

impl UiScrollbarId
{
    #[allow(clippy::wrong_self_convention)]
    fn from_f32(&self, tags: &mut Tags, value: f32)
    {
        match self
        {
            Self::Height =>
            {
                let span = HEIGHT_RANGE.end - HEIGHT_RANGE.start;
                tags.height = ((value * span as f32).floor() as i32) + HEIGHT_RANGE.start;
            },
            Self::Difficulty =>
            {
                tags.difficulty = value * DIFFICULTY_MAX;
            }
        }
    }

    #[allow(clippy::wrong_self_convention)]
    fn to_f32(&self, tags: &Tags) -> f32
    {
        match self
        {
            Self::Height =>
            {
                let span = HEIGHT_RANGE.end - HEIGHT_RANGE.start;
                (tags.height - HEIGHT_RANGE.start) as f32 / span as f32
            },
            Self::Difficulty =>
            {
                tags.difficulty / DIFFICULTY_MAX
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

const TOTAL_CHUNK_SIZE: f32 = 0.5;

const THIS_TILE_SCALING: f32 = TOTAL_CHUNK_SIZE / CHUNK_SIZE as f32;
const TILE_SCALING: f32 = THIS_TILE_SCALING / TILE_SIZE;

fn new_tile_like(
    info: &ObjectCreatePartialInfo,
    texture: &str,
    rotation: f32,
    scale: Option<Vector2<f32>>,
    pos: Vector2<f32>
) -> Object
{
    let assets = info.assets.lock();

    let model_id = assets.default_model(DefaultModel::Square);
    let model = assets.model(model_id).clone();

    let texture = assets.texture_by_name(texture).clone();

    let pos = Vector2::repeat((-TOTAL_CHUNK_SIZE + THIS_TILE_SCALING) * 0.5) + pos * THIS_TILE_SCALING;

    let position = Vector3::new(pos.x, pos.y, 0.0);

    let scale = scale.map(|x|
    {
        Vector3::new(x.x, x.y, 1.0)
    }).unwrap_or(Vector3::repeat(THIS_TILE_SCALING));

    let object_info = ObjectInfo{
        model,
        texture,
        transform: Transform{
            position,
            rotation,
            scale,
            ..Default::default()
        }
    };

    info.object_factory.create(object_info)
}

fn new_tile(
    info: &ObjectCreatePartialInfo,
    tilemap: &TileMap,
    tile: Tile,
    pos: Vector2<usize>
) -> Object
{

    let tile_info = tilemap.info(tile);
    let name = &tile_info.name;
    let rotation = -(tile.0.unwrap().rotation().to_angle() - f32::consts::FRAC_PI_2);

    new_tile_like(info, &format!("tiles/{name}.png"), rotation, None, pos.cast())
}

struct ChunkPreview
{
    tiles: Vec<Object>
}

#[derive(Debug, Clone, PartialEq)]
struct Tags
{
    last_modified: Option<SystemTime>,
    name: String,
    height: i32,
    difficulty: f32,
    others: Vec<String>
}

struct ChunkPreviewer
{
    shaders: DrawShaders,
    tilemap: TileMap,
    rules: ChunkRulesGroup,
    memory: LispMemory,
    controls: ControlsController<UiId>,
    camera: Camera,
    controller: Controller<UiId>,
    update_timer: f32,
    regenerate: bool,
    selected_textbox: Option<TextboxId>,
    chunk_code: Option<Lisp>,
    current_tags: Tags,
    preview_tags: Tags,
    preview: Option<ChunkPreview>
}

const PARENT_DIRECTORY: &str = "world_generation";

impl ChunkPreviewer
{
    fn chunk_name(name: &str) -> PathBuf
    {
        PathBuf::from(PARENT_DIRECTORY).join("chunks").join(format!("{name}.scm"))
    }

    fn compile_chunk(&mut self)
    {
        let parent_directory = PathBuf::from(PARENT_DIRECTORY);
        let filepath = parent_directory.join("chunks").join(format!("{}.scm", &self.preview_tags.name));

        if !filepath.exists()
        {
            self.chunk_code = None;
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

        let config = LispConfig{
            type_checks: true,
            memory: self.memory.clone()
        };

        match Lisp::new_with_config(config, &[&standard_code, &default_code, &chunk_code])
        {
            Ok(lisp) => self.chunk_code = Some(lisp),
            Err(err) => eprintln!("error compiling {}: {err}", &self.preview_tags.name)
        }
    }
}

struct DrawShaders
{
    normal: ShaderId,
    ui: ShaderId
}

impl YanyaApp for ChunkPreviewer
{
    type SetupInfo = ();
    type AppInfo = Option<DrawShaders>;

    fn init(info: InitPartialInfo<Self::SetupInfo>, app_info: Self::AppInfo) -> Self
    {
        let tilemap = TileMap::parse("tiles/tiles.json", "textures/tiles/").unwrap_or_else(|err|
        {
            panic!("error creating tilemap: {err}")
        }).tilemap;

        let rules = ChunkRulesGroup::load(PathBuf::from("world_generation/")).unwrap_or_else(|err|
        {
            panic!("error creating chunk_rules: {err}")
        });

        let primitives = Rc::new(ChunkGenerator::default_primitives(&tilemap));

        let memory = LispMemory::new(primitives, 256, 1 << 13);

        let controls = ControlsController::new();

        let camera = Camera::new(info.object_info.aspect(), -1.0..1.0);

        let controller = Controller::new(&info.object_info);

        let tags = Tags{
            last_modified: None,
            name: String::new(),
            height: 1,
            difficulty: 0.0,
            others: Vec::new()
        };

        let preview = None;

        Self{
            shaders: app_info.unwrap(),
            tilemap,
            rules,
            memory,
            controls,
            camera,
            controller,
            update_timer: 0.0,
            regenerate: false,
            selected_textbox: None,
            chunk_code: None,
            current_tags: tags.clone(),
            preview_tags: tags,
            preview
        }
    }

    fn update(&mut self, partial_info: UpdateBuffersPartialInfo, dt: f32)
    {
        let mut info = partial_info.to_full(&self.camera);

        let mut controls = self.controls.changed_this_frame();

        {
            let controls = &mut controls;

            let aspect = self.camera.aspect();
            let screen_body = self.controller.update(UiId::ScreenBody, UiElement{
                children_layout: UiLayout::Vertical,
                width: aspect.min(1.0).into(),
                height: aspect.recip().min(1.0).into(),
                ..Default::default()
            });

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
                    UiScrollbarId::Height =>
                    {
                        format!("height: {}", tags.height)
                    },
                    UiScrollbarId::Difficulty =>
                    {
                        format!("difficulty: {:.2}", tags.difficulty)
                    }
                };

                add_padding_horizontal(panel, UiSize::Pixels(30.0).into());
                panel.update(id(UiScrollbarPart::Text), UiElement{
                    texture: UiTexture::Text{text: description, font_size: 20},
                    ..UiElement::fit_content()
                });
            };

            update_scrollbar(UiScrollbarId::Height, &mut self.current_tags);

            add_padding_vertical(screen_body, UiSize::Pixels(10.0).into());

            update_scrollbar(UiScrollbarId::Difficulty, &mut self.current_tags);

            let mut update_button = |name: &str, id|
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
                    texture: UiTexture::Text{text: name.to_owned(), font_size: 20},
                    ..UiElement::fit_content()
                });

                button.is_mouse_inside() && controls.take_click_down()
            };

            if update_button("add tag", ButtonId::Add)
            {
                self.current_tags.others.push(String::new());
            }

            if update_button("remove tag", ButtonId::Remove)
            {
                self.current_tags.others.pop();
            }

            if update_button("regenerate", ButtonId::Regenerate)
            {
                self.regenerate = true;
            }

            let mut update_textbox = |textbox_id, text: &mut String, centered|
            {
                let id = |part|
                {
                    UiId::Textbox(textbox_id, part)
                };

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

                let name_body = parent.update(id(TextboxPartId::Body), UiElement{
                    texture: UiTexture::Solid,
                    mix: Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.3})),
                    width: UiElementSize{minimum_size: Some(UiMinimumSize::Pixels(250.0)), size: UiSize::FitChildren},
                    height: UiSize::Pixels(50.0).into(),
                    ..Default::default()
                });

                if name_body.is_mouse_inside() && controls.take_click_down()
                {
                    self.selected_textbox = Some(textbox_id);
                }

                add_padding_horizontal(name_body, UiSize::Pixels(10.0).into());

                if self.selected_textbox.as_ref().map(|x| *x == textbox_id).unwrap_or(false)
                {
                    name_body.element().mix = Some(MixColorLch::color(Lcha{l: 0.0, c: 0.0, h: 0.0, a: 0.5}));

                    text_input_handle(controls, text);
                }

                name_body.update(id(TextboxPartId::Text), UiElement{
                    texture: UiTexture::Text{text: text.clone(), font_size: 20},
                    ..UiElement::fit_content()
                });

                add_padding_horizontal(name_body, UiSize::Pixels(10.0).into());
            };

            self.current_tags.others.iter_mut().enumerate().for_each(|(index, tag)|
            {
                add_padding_vertical(screen_body, UiSize::Pixels(10.0).into());

                update_textbox(TextboxId::Tag(index as u32), tag, false);
            });

            add_padding_vertical(screen_body, UiSize::Pixels(10.0).into());

            add_padding_vertical(screen_body, UiSize::Rest(1.0).into());

            update_textbox(TextboxId::Name, &mut self.current_tags.name, true);
        }

        self.controller.create_renders(&mut info, dt);

        if self.update_timer <= 0.0
        {
            self.current_tags.last_modified = fs::metadata(Self::chunk_name(&self.current_tags.name))
                .ok()
                .and_then(|x|
                {
                    x.modified().ok()
                });
        }

        let needs_recreate = self.preview.is_none() || self.current_tags != self.preview_tags;
        let recreate_preview = self.regenerate || (self.update_timer <= 0.0 && needs_recreate);

        if recreate_preview
        {
            self.regenerate = false;

            self.preview_tags = self.current_tags.clone();

            self.compile_chunk();

            if let Some(chunk_code) = self.chunk_code.as_mut()
            {
                let mappings = &self.rules.name_mappings().text;

                let tags = self.preview_tags.others.iter().filter_map(|text|
                {
                    let equals_pos = text.chars().position(|x| x == '=')?;

                    let name = text.chars().take(equals_pos).collect::<String>();

                    let content: i32 = text.chars().skip(equals_pos + 1).collect::<String>()
                        .trim()
                        .parse()
                        .ok()?;

                    Some(WorldChunkTag::from_raw(mappings.get(&name)?, content))
                }).collect::<Vec<_>>();

                let chunk_info = ConditionalInfo{
                    height: self.preview_tags.height,
                    difficulty: self.preview_tags.difficulty,
                    tags: &tags
                };

                let mut markers = Vec::new();
                let tiles = ChunkGenerator::generate_chunk_with(
                    &chunk_info,
                    &self.rules,
                    chunk_code,
                    &mut |marker|
                    {
                        let pos = marker.pos.pos();

                        let mut pos: Vector2<f32> = Vector2::new(pos.x, pos.y).cast();

                        let (texture, scale, rotation) = match marker.kind
                        {
                            MarkerKind::Enemy{name} =>
                            {
                                let try_texture = format!("normal/enemy/{name}/body.png");
                                let texture = if PathBuf::from("textures").join(&try_texture).exists()
                                {
                                    try_texture
                                } else
                                {
                                    "normal/enemy/zob/body.png".to_owned()
                                };

                                (texture, None, 0.0)
                            },
                            MarkerKind::Furniture{name} => (format!("normal/furniture/{name}.png"), None, 0.0),
                            MarkerKind::Door{rotation: tile_rotation, width, ..} =>
                            {
                                let rotation = tile_rotation.to_angle() + f32::consts::PI;
                                let scale = Vector2::new(width as f32, 0.2) * THIS_TILE_SCALING;

                                let offset = (width as f32 - 1.0) * 0.5;

                                match tile_rotation
                                {
                                    TileRotation::Left => pos.x += offset,
                                    TileRotation::Right => pos.x -= offset,
                                    TileRotation::Down => pos.y += offset,
                                    TileRotation::Up => pos.y -= offset,
                                }

                                ("normal/furniture/metal_door1.png".to_owned(), Some(scale), rotation)
                            },
                            MarkerKind::Light{strength, offset} =>
                            {
                                pos += offset.xy() / TILE_SIZE;

                                ("normal/circle_transparent.png".to_owned(), Some(Vector2::repeat(strength * TILE_SCALING)), 0.0)
                            }
                        };

                        markers.push(new_tile_like(&info.partial, &texture, rotation, scale, pos));
                    }
                );

                match tiles
                {
                    Ok(x) =>
                    {
                        self.preview = Some(ChunkPreview{
                            tiles: x.flat_slice_iter(0).filter_map(|(pos, tile)|
                            {
                                let pos = pos.pos;

                                if tile.is_none()
                                {
                                    return None;
                                }

                                Some(new_tile(&info.partial, &self.tilemap, *tile, Vector2::new(pos.x, pos.y)))
                            }).chain(markers).collect()
                        });
                    },
                    Err(err) => eprintln!("{err} in ({})", &self.preview_tags.name)
                }
            }

            self.update_timer = 0.5;
        }

        self.update_timer -= dt;

        if let Some(preview) = self.preview.as_mut()
        {
            preview.tiles.iter_mut().for_each(|x| x.update_buffers(&mut info));
        }

        self.controller.update_buffers(&mut info);

        self.controls.consume_changed(controls).for_each(drop);
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

        self.controller.draw(&mut info);
    }

    fn resize(&mut self, aspect: f32)
    {
        self.camera.resize(aspect);
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

    App::<ChunkPreviewer>::new()
        .with_title("chunk preview")
        .with_textures_path("textures")
        .with_shaders(shaders)
        .with_app_init(Some(DrawShaders{normal, ui}))
        .with_clear_color([0.4, 0.4, 0.45])
        .run();
}
