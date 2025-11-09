#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform FillInfo{
    vec4 other_color;
    vec4 full_color;
    vec4 empty_color;
    float other_mix;
    int flags;
    float amount;
} fill;

vec4 with_mix(vec4 color)
{
    vec4 target_mix = ((fill.flags >> 1) & 1) == 1 ? vec4(color.rgb, fill.other_color.a) : fill.other_color;
    vec4 other_color = (fill.flags & 1) == 1 ? vec4(target_mix.rgb, min(color.a, target_mix.a)) : target_mix;

    return mix(color, other_color, fill.other_mix);
}

void main()
{
    vec4 color = texture(tex, tex_coords);

    vec4 filled_color = (((fill.flags >> 2) & 1) == 1 ? tex_coords.x : (1.0 - tex_coords.y)) > fill.amount ? fill.empty_color : fill.full_color;

    f_color = with_mix(color.xyz == vec3(0.0) ? vec4(filled_color.xyz, filled_color.a * color.a) : color);
}
