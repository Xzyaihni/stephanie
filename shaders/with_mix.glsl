#include "with_mix_simple.glsl"

vec4 with_mix(vec4 color)
{
    return with_mix_simple(with_palette(color));
}
