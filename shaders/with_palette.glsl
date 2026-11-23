float linear_to_srgb(float x)
{
    return x <= 0.0031308 ? x * 12.92 : pow(x, 1.0 / 2.4) * 1.055 - 0.055;
}

vec4 with_palette(vec4 color)
{
    int colors_per_palette = ?;
    int palette_step = 255 / (colors_per_palette - 1);

    vec3 palette_color = PALETTES[(int(linear_to_srgb(color.r) * 255.0) + 5) / palette_step];

    bool is_paletted = (info.flags >> ?) & 1;
    bool is_grayscale = (color.r == color.g) && (color.r == color.b);

    return (is_paletted && is_grayscale) ? vec4(palette_color, color.a) : color;
}
