#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 1) in float depth;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) buffer Palette{
    float palette[];
};

layout(set = 1, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform OutlineInfo{
    vec4 other_color;
    float other_mix;
    float animation;
    float outlined;
    int flags;
} info;

layout(constant_id = 0) const float DARKEN = 0.0;
layout(constant_id = 1) const float BLEND_RED = 0.0;
layout(constant_id = 2) const float BLEND_GREEN = 0.0;
layout(constant_id = 3) const float BLEND_BLUE = 0.0;

const vec3 BACKGROUND_COLOR = vec3(0.831, 0.941, 0.988);

#include "with_palette.glsl"
#include "with_mix.glsl"


void main()
{
    vec4 color = texture(tex, tex_coords);
    color = with_mix(color);

    vec3 blended_color = mix(color.xyz, BACKGROUND_COLOR, depth);
    vec3 darkened_color = mix(blended_color, vec3(BLEND_RED, BLEND_GREEN, BLEND_BLUE), DARKEN);

    f_color = vec4(darkened_color, color.w);
}
