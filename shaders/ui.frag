#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform OutlineInfo{
    vec3 other_color;
    float other_mix;
    bool outlined;
} outline;

void main()
{
    vec4 color = texture(tex, tex_coords);

    if (outline.other_mix != 0.0)
    {
        color = mix(color, vec4(outline.other_color, color.w), outline.other_mix);
    }

    if (outline.outlined)
    {
        color = mix(color, vec4(vec3(1.0), color.w), 0.5);
    }

    f_color = color;
}
