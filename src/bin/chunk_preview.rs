#![allow(clippy::suspicious_else_formatting)]

use std::{
    f32,
    fs,
    ops::{Range, Deref, DerefMut},
    time::SystemTime,
    rc::Rc,
    sync::Arc,
    path::{Path, PathBuf}
};

use parking_lot::Mutex;

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
    DefaultTexture,
    TextureId,
    ShaderId,
    Shader,
    ShadersGroup,
    Object,
    ObjectInfo,
    DefaultModel,
    Control,
    App,
    YanyaApp,
    object::Texture,
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
        UiControls,
        ControlsController,
        Control as GameControl,
        ui::controller::*
    },
    common::{
        with_z,
        with_error,
        some_or_return,
        render_info::*,
        lisp::*,
        TileMap,
        TileMapWithTextures,
        FurnituresInfo,
        CharactersInfo,
        EnemiesInfo,
        furniture_creator,
        colors::Lcha,
        world::{TILE_SIZE, CHUNK_VISUAL_SIZE, Tile, TileRotation}
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
    Rotation,
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
    Seed,
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

fn new_tile_like(
    info: &ObjectCreatePartialInfo,
    texture: TextureId,
    rotation: f32,
    scale: Option<Vector2<f32>>,
    pos: Vector2<f32>
) -> Object
{
    let texture = info.assets.lock().texture(texture).clone();

    new_tile_like_inner(info, texture, rotation, scale, pos)
}

fn new_tile_like_inner(
    info: &ObjectCreatePartialInfo,
    texture: Arc<Mutex<Texture>>,
    rotation: f32,
    scale: Option<Vector2<f32>>,
    pos: Vector2<f32>
) -> Object
{
    let assets = info.assets.lock();

    let model_id = assets.default_model(DefaultModel::Square);
    let model = assets.model(model_id).clone();

    let pos = Vector2::repeat((-CHUNK_VISUAL_SIZE + TILE_SIZE) * 0.5) + pos;

    let position = Vector3::new(pos.x, pos.y, 0.0);

    let scale = scale.map(|x|
    {
        Vector3::new(x.x, x.y, 1.0)
    }).unwrap_or(Vector3::repeat(TILE_SIZE));

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
    info: &mut ObjectCreatePartialInfo,
    tilemap: &TileMapWithTextures,
    tile: Tile,
    pos: Vector2<usize>
) -> Object
{

    let tile_info = tilemap.tilemap.info(tile);
    let rotation = -(tile.0.unwrap().rotation().to_angle() - f32::consts::FRAC_PI_2);

    let search_texture = tile_info.get_weighted_texture().unwrap();
    let image = tilemap.textures.iter()
        .find_map(|x| x.iter().find(|(x, _)| *x == search_texture))
        .unwrap()
        .1
        .clone();

    let texture = Texture::new(info.builder_wrapper.resource_uploader_mut(), image.into());

    new_tile_like_inner(info, Arc::new(Mutex::new(texture)), rotation, None, pos.cast() * TILE_SIZE)
}

struct ChunkPreview
{
    tiles: Vec<Object>
}

#[derive(Debug, Clone, PartialEq)]
struct ModifiedWatcher<T>
{
    paths: Vec<PathBuf>,
    last_modified: Vec<Option<SystemTime>>,
    value: T
}

impl<T> Deref for ModifiedWatcher<T>
{
    type Target = T;

    fn deref(&self) -> &Self::Target
    {
        &self.value
    }
}

impl<T> DerefMut for ModifiedWatcher<T>
{
    fn deref_mut(&mut self) -> &mut Self::Target
    {
        &mut self.value
    }
}

fn modified_time(path: &Path) -> Option<SystemTime>
{
    if !path.exists()
    {
        eprintln!("cant find path: {}", path.display());
        return None;
    }

    let this_metadata = match fs::metadata(path)
    {
        Ok(x) => x,
        Err(err) =>
        {
            eprintln!("modified time access error: {err}");
            return None;
        }
    };

    if this_metadata.is_dir()
    {
        match fs::read_dir(path)
        {
            Ok(x) =>
            {
                x.fold(None, |acc, x|
                {
                    let modified_time = match x
                    {
                        Ok(x) => modified_time(&x.path()),
                        Err(err) =>
                        {
                            eprintln!("dir entry error: {err}");
                            None
                        }
                    };

                    if let Some(modified) = modified_time
                    {
                        if let Some(acc) = acc
                        {
                            Some(if modified > acc { modified } else { acc })
                        } else
                        {
                            Some(modified)
                        }
                    } else
                    {
                        acc
                    }
                })
            },
            Err(err) =>
            {
                eprintln!("read dir error: {err}");

                None
            }
        }
    } else
    {
        match this_metadata.modified()
        {
            Ok(x) => Some(x),
            Err(err) =>
            {
                eprintln!("modified access error: {err}");

                None
            }
        }
    }
}

impl<T> ModifiedWatcher<T>
{
    fn new(path: impl Into<PathBuf>, value: T) -> Self
    {
        let path = path.into();

        Self::new_many(vec![path], value)
    }

    fn new_many(paths: Vec<PathBuf>, value: T) -> Self
    {
        let last_modified: Vec<_> = paths.iter().map(|x| modified_time(&x)).collect();

        Self{
            paths,
            last_modified,
            value
        }
    }

    fn modified_check(&mut self) -> bool
    {
        self.paths.iter().zip(self.last_modified.iter_mut()).fold(false, |modified, (path, last_modified)|
        {
            let new_modified_time = modified_time(path);

            let changed = new_modified_time != *last_modified;

            if changed
            {
                *last_modified = new_modified_time;
            }

            modified || changed
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Tags
{
    name: String,
    height: i32,
    difficulty: f32,
    rotation: TileRotation,
    seed: String,
    others: Vec<String>
}

struct AssetsDependent
{
    tilemap: Option<(LispMemory, TileMapWithTextures)>,
    furniture: FurnituresInfo,
    enemies: (CharactersInfo, EnemiesInfo),
}

impl AssetsDependent
{
    fn new(info: &ObjectCreatePartialInfo) -> Self
    {
        let tilemap = {
            let tilemap = with_error(TileMap::parse("info/tiles.json", "textures/tiles/"));

            tilemap.map(|tilemap|
            {
                let primitives = Rc::new(ChunkGenerator::default_primitives(&tilemap.tilemap));

                let memory = LispMemory::new(primitives, 256, 1 << 13);

                (memory, tilemap)
            })
        };

        let furniture = FurnituresInfo::parse(&info.assets.lock(), "normal/furniture", "info/furnitures.json");

        let enemies = {
            let mut characters = CharactersInfo::new();
            let enemies = EnemiesInfo::parse(&info.assets.lock(), &mut characters, "normal/enemy", "info/enemies.json");

            (characters, enemies)
        };

        Self{
            tilemap,
            furniture,
            enemies
        }
    }

    fn reload(&mut self, info: &mut UpdateBuffersInfo)
    {
        let assets = info.partial.assets.clone();
        assets.lock().reload(info);

        *self = Self::new(&info.partial);
    }
}

struct ChunkPreviewer
{
    shaders: DrawShaders,
    assets_dependent: ModifiedWatcher<AssetsDependent>,
    rules: ChunkRulesGroup,
    controls: ControlsController<UiId>,
    camera_position: Vector2<f32>,
    camera_zoom: f32,
    camera: Camera,
    ui_camera: Camera,
    controller: Controller<UiId>,
    update_timer: f32,
    regenerate: bool,
    selected_textbox: Option<TextboxId>,
    chunk_code: Option<Lisp>,
    current_tags: ModifiedWatcher<Tags>,
    preview_tags: ModifiedWatcher<Tags>,
    preview: Option<ChunkPreview>
}

const PARENT_DIRECTORY: &str = "world_generation";

impl ChunkPreviewer
{
    fn compile_chunk(&mut self)
    {
        let memory = some_or_return!(self.assets_dependent.tilemap.as_ref()).0.clone();

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
            memory
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
        let rules = ChunkRulesGroup::load(PathBuf::from("world_generation/")).unwrap_or_else(|err|
        {
            panic!("error creating chunk_rules: {err}")
        });

        let controls = ControlsController::new();

        let camera_position = Vector2::new(-0.5, 0.0);
        let camera_zoom = 3.0;
        let mut camera = Camera::new(info.object_info.aspect(), -1.0..1.0);
        camera.rescale(camera_zoom);
        camera.set_position(with_z(camera_position, 0.0).into());

        let ui_camera = Camera::new(info.object_info.aspect(), -1.0..1.0);

        let controller = Controller::new(&info.object_info);

        let tags = ModifiedWatcher::new(PARENT_DIRECTORY, Tags{
            name: String::new(),
            height: 1,
            difficulty: 0.0,
            rotation: TileRotation::Up,
            seed: String::new(),
            others: Vec::new()
        });

        let preview = None;

        let assets_dependent = ModifiedWatcher::new_many(
            vec!["textures".into(), "info".into(), "lisp".into()],
            AssetsDependent::new(&info.object_info)
        );

        Self{
            shaders: app_info.unwrap(),
            assets_dependent,
            rules,
            controls,
            camera_position,
            camera_zoom,
            camera,
            ui_camera,
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
        let mut info = partial_info.to_full(&self.ui_camera);

        if self.update_timer <= 0.0
        {
            if self.current_tags.modified_check()
            {
                eprintln!("hot reloading chunk `{}`", &self.current_tags.name);
                self.regenerate = true;
            }

            if self.assets_dependent.modified_check()
            {
                eprintln!("hot reloading assets");
                self.assets_dependent.reload(&mut info);

                self.regenerate = true;
            }
        }

        let mut controls = self.controls.changed_this_frame();

        {
            let controls = &mut controls;

            let aspect = self.ui_camera.aspect();
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
                    texture: UiTexture::Text{text: name.to_owned(), font_size: 20},
                    ..UiElement::fit_content()
                });

                button.is_mouse_inside() && controls.take_click_down()
            };

            let mut update_textbox = |controls: &mut UiControls<_>, textbox_id, text: &mut String, centered|
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

            if update_button(controls, &format!("{:?}", self.current_tags.rotation), ButtonId::Rotation)
            {
                self.current_tags.rotation = self.current_tags.rotation.rotate_clockwise();
            }

            if update_button(controls, "regenerate", ButtonId::Regenerate)
            {
                self.regenerate = true;
            }

            add_padding_vertical(screen_body, UiSize::Pixels(10.0).into());

            update_textbox(controls, TextboxId::Seed, &mut self.current_tags.seed, false);

            add_padding_vertical(screen_body, UiSize::Pixels(20.0).into());

            if update_button(controls, "add tag", ButtonId::Add)
            {
                self.current_tags.others.push(String::new());
            }

            if update_button(controls, "remove tag", ButtonId::Remove)
            {
                self.current_tags.others.pop();
            }

            self.current_tags.others.iter_mut().enumerate().for_each(|(index, tag)|
            {
                add_padding_vertical(screen_body, UiSize::Pixels(10.0).into());

                update_textbox(controls, TextboxId::Tag(index as u32), tag, false);
            });

            add_padding_vertical(screen_body, UiSize::Rest(1.0).into());

            update_textbox(controls, TextboxId::Name, &mut self.current_tags.name, true);
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
                    rotation: self.preview_tags.rotation,
                    tags: &tags
                };

                if !self.preview_tags.seed.is_empty()
                {
                    let seed = self.preview_tags.seed.bytes().fold(0_u64, |acc, x| acc + x as u64);

                    fastrand::seed(seed);
                }

                let mut markers = Vec::new();
                let tiles = ChunkGenerator::generate_chunk_with(
                    &chunk_info,
                    &self.rules,
                    &self.preview_tags.name,
                    chunk_code,
                    &mut |marker|
                    {
                        let from_name = |name|
                        {
                            info.partial.assets.lock().texture_id(name)
                        };

                        let default_texture = info.partial.assets.lock().default_texture(DefaultTexture::Solid);

                        let pos = marker.pos.pos();

                        let mut pos: Vector2<f32> = Vector2::new(pos.x, pos.y).cast() * TILE_SIZE;

                        let (texture, scale, rotation) = match marker.kind
                        {
                            MarkerKind::Enemy{name} =>
                            {
                                let enemies = &self.assets_dependent.enemies;
                                let texture = enemies.1.get_id(&name.replace('_', "")).map(|id|
                                {
                                    enemies.0.get(enemies.1.get(id).character).normal
                                }).unwrap_or(default_texture);

                                (texture, None, 0.0)
                            },
                            MarkerKind::Furniture{name, rotation: tile_rotation} =>
                            {
                                let furnitures = &self.assets_dependent.furniture;
                                let (texture, scale, rotation) = furnitures.get_id(&name.replace('_', " ")).map(|id|
                                {
                                    let furniture = furnitures.get(id);

                                    let transform = Transform{
                                        rotation: tile_rotation.flip_y().to_angle(),
                                        scale: with_z(furniture.scale, 1.0),
                                        ..Default::default()
                                    };

                                    pos += furniture_creator::furniture_position(furniture, tile_rotation);

                                    let (closest, transform) = rotating_info(transform, furniture.collision, &furniture.textures);

                                    (closest, transform.scale.xy(), transform.rotation)
                                }).unwrap_or((default_texture, Vector2::repeat(TILE_SIZE), 0.0));

                                (texture, Some(scale), rotation)
                            },
                            MarkerKind::Door{rotation: tile_rotation, width, ..} =>
                            {
                                let rotation = tile_rotation.to_angle() + f32::consts::PI;
                                let scale = Vector2::new(width as f32, 0.2) * TILE_SIZE;

                                let offset = (width as f32 - 1.0) * 0.5 * TILE_SIZE;

                                match tile_rotation
                                {
                                    TileRotation::Left => pos.x += offset,
                                    TileRotation::Right => pos.x -= offset,
                                    TileRotation::Down => pos.y -= offset,
                                    TileRotation::Up => pos.y += offset
                                }

                                (from_name("normal/furniture/metal_door1.png"), Some(scale), rotation)
                            },
                            MarkerKind::Light{strength, offset} =>
                            {
                                pos += offset.xy();

                                (from_name("normal/circle_transparent.png"), Some(Vector2::repeat(strength)), 0.0)
                            }
                        };

                        markers.push(new_tile_like(&info.partial, texture, rotation, scale, pos));
                    }
                );

                let tilemap = &some_or_return!(self.assets_dependent.tilemap.as_ref()).1;

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

                                Some(new_tile(&mut info.partial, tilemap, *tile, Vector2::new(pos.x, pos.y)))
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

        if controls.is_click_down() && !controls.is_click_taken()
        {
            self.selected_textbox = None;
        }

        self.controls.consume_changed(controls).for_each(drop);

        let speed = 0.5 * dt;
        let zoom_speed = 1.2 * dt;

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
            self.camera_zoom = (self.camera_zoom - zoom_speed).max(0.0);
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

        self.controller.draw(&mut info);
    }

    fn resize(&mut self, aspect: f32)
    {
        self.ui_camera.resize(aspect);
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
