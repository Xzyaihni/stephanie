#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 1) in float depth;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D face_tex;
layout(set = 1, binding = 0) uniform sampler2D eyes_closed_tex;
layout(set = 2, binding = 0) uniform sampler2D eyes_normal_tex;

layout(set = 3, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform CharacterShaderInfoRaw{
    vec4 other_color;
    vec2 aspect;
    vec2 face_offset;
    vec2 eyes_offset;
    float other_mix;
    int flags;
    float animation;
    float outlined;
} info;

const vec3 BACKGROUND_COLOR = vec3(0.831, 0.941, 0.988);

const float LINE_WIDTH = 0.5;

vec4 with_mix(vec4 color)
{
    vec4 target_mix = ((info.flags >> 1) & 1) == 1 ? vec4(color.rgb, info.other_color.a) : info.other_color;
    vec4 other_color = (info.flags & 1) == 1 ? vec4(target_mix.rgb, min(color.a, target_mix.a)) : target_mix;

    return mix(color, other_color, info.other_mix);
}

vec4 blend(vec4 src, vec4 color)
{
    return vec4(mix(src.rgb, color.rgb, color.a), min(src.a + color.a, 1.0));
}

vec4 get_texture(sampler2D tex, vec2 pos)
{
    return (pos.x > 1.0 || pos.x < 0.0 || pos.y > 1.0 || pos.y < 0.0) ? vec4(0.0) : texture(tex, pos);
}

void main()
{
    vec4 color = texture(tex, tex_coords);

    int draw_eyes = (info.flags >> 2) & 1;
    int left_closed = (info.flags >> 3) & 1;
    int right_closed = (info.flags >> 4) & 1;

    vec2 face_coords = tex_coords / info.aspect;

    if (info.aspect.x < info.aspect.y)
    {
        face_coords.y -= (1.0 - info.aspect.y) * 0.5;
    } else
    {
        face_coords.x -= (1.0 - info.aspect.x) * 0.5;
    }

    face_coords -= info.face_offset;

    vec4 face_color = get_texture(face_tex, face_coords);

    vec2 eyes_coords = face_coords - info.eyes_offset;

    vec4 eyes_closed_color = get_texture(eyes_closed_tex, eyes_coords);
    vec4 eyes_normal_color = get_texture(eyes_normal_tex, eyes_coords);

    vec4 eyes_color = (tex_coords.y > 0.5 ? left_closed : right_closed) == 1 ? eyes_closed_color : eyes_normal_color;

    color = blend(blend(color, face_color), draw_eyes == 1 ? eyes_color : vec4(0.0));

    color = with_mix(color);

    float coord = gl_FragCoord.x + gl_FragCoord.y;
    float outline_lines = mod(coord * 0.02 + info.animation * 0.5, LINE_WIDTH);
    float outline_mask = outline_lines > (LINE_WIDTH * 0.5) ? 0.5 : 0.0;
    color = mix(color, vec4(vec3(outline_mask), color.a), info.outlined * 0.5);

    f_color = vec4(mix(color.xyz, BACKGROUND_COLOR, depth), color.w);
}
