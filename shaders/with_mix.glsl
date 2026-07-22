const int COLORS_PER_PALETTE = 9;
const int PALETTES_LENGTH = 5;

const int PALETTE_STEP = 255 / (COLORS_PER_PALETTE - 1);

const int PALLETTES_ARRAY_SIZE = PALETTES_LENGTH * COLORS_PER_PALETTE;

const vec3 PALETTES[PALLETTES_ARRAY_SIZE] = vec3[PALLETTES_ARRAY_SIZE](
    vec3(0.371, 0.045, 0.855), vec3(0.527, 0.125, 0.982), vec3(0.855, 0.305, 1.000), vec3(0.956, 0.332, 0.847), vec3(1.000, 0.407, 0.745), vec3(1.000, 0.617, 0.730), vec3(1.000, 0.784, 0.863), vec3(0.855, 0.305, 1.000), vec3(1.000, 0.407, 0.745),
    vec3(0.168, 0.100, 0.716), vec3(0.159, 0.171, 0.947), vec3(0.168, 0.434, 0.905), vec3(0.254, 0.745, 1.000), vec3(0.296, 0.896, 1.000), vec3(0.597, 0.982, 0.956), vec3(0.871, 0.991, 0.973), vec3(0.429, 0.366, 0.888), vec3(0.665, 0.462, 1.000),
    vec3(0.015, 0.521, 0.479), vec3(0.023, 0.610, 0.342), vec3(0.091, 0.815, 0.162), vec3(0.279, 0.913, 0.162), vec3(0.533, 1.000, 0.209), vec3(0.745, 1.000, 0.371), vec3(0.922, 1.000, 0.687), vec3(0.456, 0.631, 0.184), vec3(0.651, 0.610, 0.216),
    vec3(0.474, 0.002, 0.287), vec3(0.597, 0.027, 0.231), vec3(0.807, 0.034, 0.067), vec3(1.000, 0.076, 0.072), vec3(1.000, 0.138, 0.068), vec3(1.000, 0.296, 0.153), vec3(1.000, 0.730, 0.515), vec3(0.939, 0.223, 0.287), vec3(0.930, 0.337, 0.305),
    vec3(0.109, 0.012, 0.223), vec3(0.147, 0.030, 0.418), vec3(0.191, 0.076, 0.888), vec3(0.127, 0.162, 1.000), vec3(0.165, 0.309, 1.000), vec3(0.202, 0.418, 1.000), vec3(0.485, 0.730, 1.000), vec3(0.287, 0.287, 0.922), vec3(0.479, 0.386, 1.000)
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
