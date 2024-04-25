use std::{
    time::{Instant, Duration},
    ops::RangeInclusive
};


pub trait Animatable<T: Clone>
{
    fn set(&mut self, id: T, value: f32);
}

#[derive(Debug, Clone)]
pub enum ValueAnimation
{
	Linear,
	EaseIn(f32),
	EaseOut(f32)
}

impl ValueAnimation
{
	pub fn apply(&self, value: f32) -> f32
	{
		let value = value.clamp(0.0, 1.0);

		match self
		{
			Self::Linear => value,
			Self::EaseIn(strength) => value.powf(*strength),
			Self::EaseOut(strength) => 1.0 - (1.0 - value).powf(*strength)
		}
	}

    pub fn reversed(&self) -> Self
    {
        match self
        {
            Self::Linear => Self::Linear,
            Self::EaseIn(x) => Self::EaseOut(*x),
            Self::EaseOut(x) => Self::EaseIn(*x)
        }
    }
}

pub enum AnimationState
{
    Playing,
    Over
}

#[derive(Debug, Clone)]
pub struct AnimatedValue<T>
{
    pub id: T,
    pub range: RangeInclusive<f32>,
    pub curve: ValueAnimation,
    pub duration: RangeInclusive<f32>
}

impl<T> AnimatedValue<T>
{
    fn validate(&self)
    {
        assert!(self.duration.start() < self.duration.end());

        assert!(*self.duration.start() >= 0.0);
        assert!(*self.duration.end() <= 1.0);
    }

    fn reverse(&mut self)
    {
        self.curve = self.curve.reversed();
        self.range = *self.range.end()..=*self.range.start();
        self.duration = (1.0 - *self.duration.end())..=(1.0 - *self.duration.start());
    }

    fn total_duration(&self) -> f32
    {
        self.duration.end() - self.duration.start()
    }
}

#[derive(Debug, Clone)]
pub struct Animator<T>
{
    values: Vec<AnimatedValue<T>>,
    duration: Duration,
    start: Instant,
    is_playing: bool
}

#[allow(dead_code)]
impl<T: Clone> Animator<T>
{
    pub fn new(values: Vec<AnimatedValue<T>>, duration: Duration) -> Self
    {
        values.iter().for_each(|value| value.validate());

        // start at the end
        Self{values, duration, start: Instant::now() - duration, is_playing: false}
    }

    pub fn reset(&mut self)
    {
        self.start = Instant::now();
        self.is_playing = true;
    }

    pub fn reversed(&self) -> Self
    where
        T: Clone
    {
        let mut this = (*self).clone();

        this.values.iter_mut().for_each(AnimatedValue::reverse);

        this
    }

    pub fn animate(&mut self, animatable: &mut impl Animatable<T>) -> AnimationState
    {
        let timepoint = (self.start.elapsed().as_secs_f32() / self.duration.as_secs_f32())
            .min(1.0);

        // how many combinations of point, value, scaled and wutever else can i come up with
        self.values.iter().for_each(|anim_value|
        {
            let scaled_point = {
                let duration = &anim_value.duration;

                let clamped = timepoint.clamp(*duration.start(), *duration.end());

                (clamped - duration.start()) / anim_value.total_duration()
            };

            let point = anim_value.curve.apply(scaled_point);

            let value = Self::lerp(&anim_value.range, point);

            animatable.set(anim_value.id.clone(), value);
        });

        if timepoint >= 1.0
        {
            self.is_playing = false;

            AnimationState::Over
        } else
        {
            AnimationState::Playing
        }
    }

    pub fn is_playing(&self) -> bool
    {
        self.is_playing
    }

    pub fn set_duration(&mut self, duration: Duration)
    {
        self.duration = duration;
    }

    fn lerp(range: &RangeInclusive<f32>, a: f32) -> f32
    {
        range.start() * (1.0 - a) + range.end() * a
    }
}
