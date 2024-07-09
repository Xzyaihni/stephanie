#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform OutlineInfo{
    vec3 other_color;
    float other_mix;
    float animation;
    float outlined;
} outline;

void main()
{
    vec4 color = texture(tex, tex_coords);

    f_color = mix(color, vec4(outline.other_color, 1.0), outline.other_mix);
}
