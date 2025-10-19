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

const float line_width = 0.5;

vec4 with_mix(vec4 color)
{
    vec4 other_color = outline.keep_transparency ? vec4(outline.other_color.rgb, min(color.a, outline.other_color.a)) : outline.other_color;

    return mix(color, other_color, outline.other_mix);
}

void main()
{
    vec4 color = texture(tex, tex_coords);

    color = with_mix(color);

    float coord = gl_FragCoord.x + gl_FragCoord.y;
    float outline_lines = mod(coord * 0.02 + outline.animation * 0.5, line_width);
    float outline_mask = outline_lines > (line_width * 0.5) ? 0.5 : 0.0;
    color = mix(color, vec4(vec3(outline_mask), color.a), outline.outlined * 0.5);

    f_color = vec4(mix(color.xyz, background_color, depth), color.w);
}
