#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform OutlineInfo{
    vec4 other_color;
    float other_mix;
    int flags;
    int palette;
} info;

#include "with_mix.glsl"


void main()
{
    vec4 color = texture(tex, tex_coords);

    f_color = with_mix(color);
}
