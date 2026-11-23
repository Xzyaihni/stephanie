float linear_to_srgb(float x)
{
    return x <= 0.0031308 ? x * 12.92 : pow(x, 1.0 / 2.4) * 1.055 - 0.055;
}

vec3 palette_at(int index)
{
    return vec3(palette[index * 3], palette[index * 3 + 1], palette[index * 3 + 2]);
}

vec4 with_palette(vec4 color)
{
    int colors_per_palette = palette.length();
    int palette_step = 255 / (colors_per_palette - 1);

    float srgb_brightness = linear_to_srgb(color.r);
    int srgb_brightness_byte = int(srgb_brightness * 255.0);

    // adding a small number to make sure the conversion and rounding down didnt change the step
    int palette_index = (srgb_brightness_byte + 5) / palette_step;

    vec3 palette_color = palette_at(palette_index);

    bool is_paletted = ((info.flags >> 2) & 1) == 1;
    bool is_grayscale = (color.r == color.g) && (color.r == color.b);

    return (is_paletted && is_grayscale) ? vec4(palette_color, color.a) : color;
}
