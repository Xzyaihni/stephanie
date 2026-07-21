vec4 with_mix_simple(vec4 color)
{
    vec4 target_mix = ((info.flags >> 1) & 1) == 1 ? vec4(color.rgb, info.other_color.a) : info.other_color;
    vec4 other_color = (info.flags & 1) == 1 ? vec4(target_mix.rgb, min(color.a, target_mix.a)) : target_mix;

    return mix(color, other_color, info.other_mix);
}
