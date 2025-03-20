#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 1) in float depth;

layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform OutlineInfo{
    vec4 other_color;
    float other_mix;
    float animation;
    float outlined;
    bool keep_transparency;
} outline;

const vec3 background_color = vec3(0.831, 0.941, 0.988);

vec4 with_mix(vec4 color)
{
    vec4 other_color = outline.keep_transparency ? vec4(outline.other_color.xyz, color.a) : outline.other_color;

    return mix(color, other_color, outline.other_mix);
}

void main()
{
    vec4 color = texture(tex, tex_coords);

    color = with_mix(color);

    vec3 animation_color = sin(vec3(3.0, 4.0, 2.0) * outline.animation) * vec3(0.5, 0.1, 0.3);
    vec3 outline_color = tex_coords.xyx + animation_color + vec3(0.3, 0.4, 0.2);
    color = mix(color, vec4(outline_color, color.w), outline.outlined * 0.5);

    f_color = vec4(mix(color.xyz, background_color, depth), color.w);
}
