use std::f32;

use crate::common::{lerp, EaseOut};

pub use image::{Rgb, Rgba};


pub fn srgb_to_linear(x: [f32; 3]) -> [f32; 3]
{
    fn f(value: f32) -> f32
    {
        if value <= 0.04045
        {
            value / 12.92
        } else
        {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    x.map(f)
}

#[derive(Debug, Clone, Copy)]
pub struct Laba
{
    pub l: f32,
    pub a: f32,
    pub b: f32,
    pub alpha: f32
}

impl Laba
{
    pub fn no_alpha(self) -> Lab
    {
        Lab{
            l: self.l,
            a: self.a,
            b: self.b
        }
    }

    pub fn blend(self, other: Laba) -> Laba
    {
        if self.alpha == 0.0
        {
            return other;
        } else if other.alpha == 0.0
        {
            return self;
        }

        // or u could express this as lerp(self.alpha, 1.0, other.alpha)
        let alpha = (other.alpha + self.alpha * (1.0 - other.alpha)).clamp(0.0, 1.0);

        let mix = |a, b|
        {
            lerp(a * self.alpha, b, other.alpha) / alpha
        };

        Self{
            l: mix(self.l, other.l),
            a: mix(self.a, other.a),
            b: mix(self.b, other.b),
            alpha
        }
    }
}

impl From<Lab> for Laba
{
    fn from(lab: Lab) -> Self
    {
        Self{
            l: lab.l,
            a: lab.a,
            b: lab.b,
            alpha: 1.0
        }
    }
}

impl From<Lab> for Rgb<u8>
{
    fn from(value: Lab) -> Self
    {
        let rgb = Rgb::from(value);

        let inner: Vec<u8> = rgb.0.into_iter().map(|x: f32|
        {
            (x.clamp(0.0, 1.0) * u8::MAX as f32) as u8
        }).collect();

        let inner: [u8; 3] = inner.try_into().unwrap();

        Rgb::from(inner)
    }
}

impl From<Lab> for Rgb<f32>
{
    fn from(value: Lab) -> Self
    {
        let xyz = Xyz::from(value);

        Self::from(xyz)
    }
}

impl From<Lab> for Xyz
{
    fn from(value: Lab) -> Self
    {
        let l_rev = (value.l + 16.0) / 116.0;

        let delta = 6.0_f32 / 29.0;
        let f_inv = |value: f32|
        {
            if value > delta
            {
                value.powi(3)
            } else
            {
                3.0 * delta.powi(2) * (value - (4.0 / 29.0))
            }
        };

        let x = 95.0489 * f_inv(l_rev + value.a / 500.0);
        let y = 100.0 * f_inv(l_rev);
        let z = 108.884 * f_inv(l_rev - value.b / 200.0);

        Self{x, y, z}
    }
}

impl From<Rgba<f32>> for Laba
{
    fn from(value: Rgba<f32>) -> Self
    {
        let [r, g, b, _a] = value.0;
        let rgb = Rgb::from([r, g, b]);

        let lab = Lab::from(rgb);

        Self{
            alpha: value.0[3].clamp(0.0, 1.0),
            ..Laba::from(lab)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lch
{
    pub l: f32,
    pub c: f32,
    pub h: f32
}

impl From<Lcha> for Lch
{
    fn from(value: Lcha) -> Self
    {
        Lch{
            l: value.l,
            c: value.c,
            h: value.h
        }
    }
}

impl From<Lcha> for [u8; 3]
{
    fn from(value: Lcha) -> Self
    {
        Rgb::from(Lab::from(Lch::from(value))).0
    }
}

impl From<Lch> for [f32; 3]
{
    fn from(value: Lch) -> Self
    {
        Rgb::from(Lab::from(value)).0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lcha
{
    pub l: f32,
    pub c: f32,
    pub h: f32,
    pub a: f32
}

impl Lcha
{
    pub fn add_lightness(&mut self, l: f32)
    {
        self.l = self.with_added_lightness(l).l;
    }

    pub const fn with_added_lightness(self, l: f32) -> Self
    {
        Self{
            l: (self.l + l).clamp(0.0, 100.0),
            c: self.c,
            h: self.h,
            a: self.a
        }
    }

    pub const fn with_added_chroma(self, c: f32) -> Self
    {
        Self{
            l: self.l,
            c: (self.c + c).clamp(0.0, 100.0),
            h: self.h,
            a: self.a
        }
    }

    pub const fn with_added_hue(self, h: f32) -> Self
    {
        Self{
            l: self.l,
            c: self.c,
            h: (self.h + h) % (f32::consts::PI * 2.0),
            a: self.a
        }
    }
}

impl From<Lcha> for [f32; 4]
{
    fn from(value: Lcha) -> Self
    {
        let [r, g, b] = Lch::from(value).into();

        [r, g, b, value.a]
    }
}

impl EaseOut for Lcha
{
    fn ease_out(&self, target: Self, decay: f32, dt: f32) -> Self
    {
        Self{
            l: self.l.ease_out(target.l, decay, dt),
            c: self.c.ease_out(target.c, decay, dt),
            h: self.h.ease_out(target.h, decay, dt),
            a: self.a.ease_out(target.a, decay, dt)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Lab
{
    pub l: f32,
    pub a: f32,
    pub b: f32
}

impl Lab
{
    pub fn random() -> Self
    {
        let r = |value|
        {
            (fastrand::f32() * 2.0 - 1.0) * value
        };

        Self{l: r(25.0) + 50.0, a: r(50.0), b: r(50.0)}
    }

    pub fn distance(&self, other: Lab) -> f32
    {
        let d_l = other.l - self.l;
        let d_a = other.a - self.a;
        let d_b = other.b - self.b;

        d_l.powi(2) + d_a.powi(2) + d_b.powi(2)
    }

    pub fn map<F>(self, mut f: F) -> Self
    where
        F: FnMut(f32) -> f32
    {
        Self{
            l: f(self.l),
            a: f(self.a),
            b: f(self.b)
        }
    }

    pub fn blend(self, other: Laba) -> Lab
    {
        Self{
            l: lerp(self.l, other.l, other.alpha),
            a: lerp(self.a, other.a, other.alpha),
            b: lerp(self.b, other.b, other.alpha)
        }
    }
}

impl From<Lch> for Lab
{
    fn from(value: Lch) -> Self
    {
        Self{
            l: value.l,
            a: value.c * value.h.cos(),
            b: value.c * value.h.sin()
        }
    }
}

impl From<Xyz> for Lab
{
    fn from(value: Xyz) -> Self
    {
        let delta = 6.0_f32 / 29.0;
        let delta_cube = delta.powi(3);
        let lower_scale = 1.0 / (delta.powi(2) * 3.0);

        let f = |value: f32| -> f32
        {
            if value > delta_cube
            {
                value.cbrt()
            } else
            {
                value * lower_scale + (4.0 / 29.0)
            }
        };

        let x = f(value.x / 95.047);
        let y = f(value.y / 100.0);
        let z = f(value.z / 108.883);

        let l = 116.0 * y - 16.0;
        let a = 500.0 * (x - y);
        let b = 200.0 * (y - z);

        Self{l, a, b}
    }
}

impl From<Rgb<f32>> for Lab
{
    fn from(value: Rgb<f32>) -> Self
    {
        Xyz::from(value).into()
    }
}

#[derive(Debug, Clone, Copy)]
struct Xyz
{
    x: f32,
    y: f32,
    z: f32
}

impl Xyz
{
    pub fn map<F>(self, mut f: F) -> Self
    where
        F: FnMut(f32) -> f32
    {
        Xyz{
            x: f(self.x),
            y: f(self.y),
            z: f(self.z)
        }
    }
}

impl From<Rgb<f32>> for Xyz
{
    fn from(value: Rgb<f32>) -> Self
    {
        let [r, g, b] = srgb_to_linear(value.0).map(|x| x * 100.0);

        let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
        let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
        let z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b;

        Self{x, y, z}
    }
}

impl From<Xyz> for Rgb<f32>
{
    fn from(value: Xyz) -> Self
    {
        let value = value.map(|x| x / 100.0);

        let f = |a: f32, b: f32, c: f32| -> f32
        {
            let x = value.x * a + value.y * b + value.z * c;

            if x > 0.0031308
            {
                1.055 * (x.powf(1.0 / 2.4)) - 0.055
            } else
            {
                12.92 * x
            }.clamp(0.0, 1.0)
        };

        let r = f(3.2406, -1.5372, -0.4986);
        let g = f(-0.9689, 1.8758, 0.0415);
        let b = f(0.0557, -0.2040, 1.0570);

        Rgb::from([r, g, b])
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    fn close_enough(a: f32, b: f32)
    {
        assert!((a - b).abs() < 0.001, "a: {}, b: {}", a, b);
    }

    #[test]
    fn xyz_to_lab()
    {
        let xyz = Xyz{x: 0.5, y: 0.0, z: 0.0};

        let lab = Lab::from(xyz);

        close_enough(lab.l, 0.0);
        close_enough(lab.a, 20.482);
        close_enough(lab.b, 0.0);

        let xyz = Xyz{x: 0.1, y: 0.5, z: 0.9};

        let lab = Lab::from(xyz);

        close_enough(lab.l, 4.516);
        close_enough(lab.a, -15.371);
        close_enough(lab.b, -5.086);

        let rgb = Rgb::from([0.5, 0.2, 0.8]);

        let xyz = Xyz::from(rgb);

        close_enough(xyz.x, 20.907);
        close_enough(xyz.y, 11.278);
        close_enough(xyz.z, 58.190);
    }

    #[test]
    fn rgb_to_lab()
    {
        let rgb = Rgb::from([0.3, 0.6, 0.9]);

        let lab = Lab::from(rgb);

        close_enough(lab.l, 61.673);
        close_enough(lab.a, 0.33);
        close_enough(lab.b, -45.62);

        let back_rgb = Rgb::from(lab);

        rgb.0.iter().zip(back_rgb.0.iter()).for_each(|(&a, &b)|
        {
            close_enough(a, b);
        });
    }
}
