#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform OutlineInfo{
    vec4 other_color;
    float other_mix;
    bool keep_transparency;
} outline;

vec4 with_mix(vec4 color)
{
    vec4 other_color = outline.keep_transparency ? vec4(outline.other_color.rgb, min(color.a, outline.other_color.a)) : outline.other_color;

    return mix(color, other_color, outline.other_mix);
}

void main()
{
    vec4 color = texture(tex, tex_coords);

    f_color = with_mix(color);
}
