#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 1) in float depth;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;
layout(push_constant) uniform OutlineInfo {
    bool outlined;
} outline;

const vec3 background_color = vec3(0.831, 0.941, 0.988);

void main()
{
    vec4 color = texture(tex, tex_coords);

    if (outline.outlined)
    {
        color = mix(color, vec4(vec3(1.0), color.w), 0.5);
    }

    f_color = vec4(mix(color.xyz, background_color, depth), color.w);
}
