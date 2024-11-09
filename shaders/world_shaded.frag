#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 1) in float depth;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(constant_id = 0) const float DARKEN = 0.0;
layout(constant_id = 1) const float BLEND_RED = 0.0;
layout(constant_id = 2) const float BLEND_GREEN = 0.0;
layout(constant_id = 3) const float BLEND_BLUE = 0.0;

const vec3 background_color = vec3(0.831, 0.941, 0.988);

void main()
{
    vec4 color = texture(tex, tex_coords);

    vec3 blend = vec3(BLEND_RED, BLEND_GREEN, BLEND_BLUE);

    vec3 blended_color = mix(color.xyz, background_color, depth);
    vec3 darkened_color = mix(blended_color, blend, DARKEN);

    vec3 solid_color = mix(blended_color, blend, mix(DARKEN, 1.0, 0.3));
    vec3 final_color = (depth == 0.0) ? solid_color : darkened_color;

    f_color = vec4(final_color, color.w);
}
