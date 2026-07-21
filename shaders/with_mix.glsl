const int COLORS_PER_PALETTE = 9;
const int PALETTES_LENGTH = 2;

const int PALETTE_STEP = 255 / (COLORS_PER_PALETTE - 1);

const int PALLETTES_ARRAY_SIZE = PALETTES_LENGTH * COLORS_PER_PALETTE;

const vec3 PALETTES[PALLETTES_ARRAY_SIZE] = vec3[PALLETTES_ARRAY_SIZE](
    vec3(0.371, 0.045, 0.855), vec3(0.527, 0.125, 0.982), vec3(0.855, 0.305, 1.000), vec3(0.956, 0.332, 0.847), vec3(1.000, 0.407, 0.745), vec3(1.000, 0.491, 0.730), vec3(1.000, 0.784, 0.863), vec3(0.855, 0.305, 1.000), vec3(1.000, 0.407, 0.745),
    vec3(0.168, 0.100, 0.716), vec3(0.159, 0.171, 0.947), vec3(0.168, 0.434, 0.905), vec3(0.254, 0.745, 1.000), vec3(0.296, 0.896, 1.000), vec3(0.597, 0.982, 0.956), vec3(0.871, 0.991, 0.973), vec3(0.429, 0.366, 0.888), vec3(0.665, 0.462, 1.000)
);

#include "with_mix_simple.glsl"

float linear_to_srgb(float x)
{
    return x <= 0.0031308 ? x * 12.92 : pow(x, 1.0 / 2.4) * 1.055 - 0.055;
}

vec4 with_palette(vec4 color)
{
    return (info.palette != 0 && ((color.r == color.g) && (color.r == color.b))) ? vec4(PALETTES[(info.palette - 1) * COLORS_PER_PALETTE + ((int(linear_to_srgb(color.r) * 255.0) + 10) / PALETTE_STEP)], color.a) : color;
}

vec4 with_mix(vec4 color)
{
    return with_mix_simple(with_palette(color));
}
