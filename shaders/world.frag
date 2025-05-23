#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 1) in float depth;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

const vec3 background_color = vec3(0.831, 0.941, 0.988);

void main()
{
    vec4 color = texture(tex, tex_coords);

    vec3 blended_color = mix(color.xyz, background_color, depth);

    f_color = vec4(blended_color, color.w);
}
