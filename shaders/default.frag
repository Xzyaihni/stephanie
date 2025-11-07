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
    int flags;
} outline;

const vec3 BACKGROUND_COLOR = vec3(0.831, 0.941, 0.988);

const float LINE_WIDTH = 0.5;

vec4 with_mix(vec4 color)
{
    vec4 target_mix = ((outline.flags >> 1) & 1) == 1 ? vec4(color.rgb, outline.other_color.a) : outline.other_color;
    vec4 other_color = (outline.flags & 1) == 1 ? vec4(target_mix.rgb, min(color.a, target_mix.a)) : target_mix;

    return mix(color, other_color, outline.other_mix);
}

void main()
{
    vec4 color = texture(tex, tex_coords);

    color = with_mix(color);

    float coord = gl_FragCoord.x + gl_FragCoord.y;
    float outline_lines = mod(coord * 0.02 + outline.animation * 0.5, LINE_WIDTH);
    float outline_mask = outline_lines > (LINE_WIDTH * 0.5) ? 0.5 : 0.0;
    color = mix(color, vec4(vec3(outline_mask), color.a), outline.outlined * 0.5);

    f_color = vec4(mix(color.xyz, BACKGROUND_COLOR, depth), color.w);
}
