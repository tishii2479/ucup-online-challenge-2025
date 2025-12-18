#![allow(dead_code)]

#[derive(Debug, Clone, Copy)]
pub struct Rnd {
    state: u32,
}

impl Rnd {
    pub fn new(mut seed: u32) -> Self {
        if seed == 0 {
            seed = 12345678;
        }
        Self { state: seed }
    }

    #[inline(always)]
    pub fn _next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    #[inline(always)]
    pub fn nextf(&mut self) -> f64 {
        self._next() as f64 / ((1u64 << 32) as f64)
    }

    #[inline(always)]
    pub fn choice<T: Clone + Copy>(&mut self, v: &[T]) -> T {
        let idx = self.gen_index(v.len());
        v[idx]
    }

    #[inline(always)]
    pub fn gen_index(&mut self, len: usize) -> usize {
        debug_assert!(len as u64 <= 1 << 32);
        ((len as u64 * self._next() as u64) >> 32) as usize
    }

    #[inline(always)]
    pub fn gen_range(&mut self, l: usize, r: usize) -> usize {
        debug_assert!(l < r);
        debug_assert!(r as u64 <= 1 << 32);
        l + (((r - l) as u64 * self._next() as u64) >> 32) as usize
    }

    #[inline(always)]
    pub fn gen_rangef(&mut self, l: f64, r: f64) -> f64 {
        debug_assert!(l <= r);
        l + self._next() as f64 * ((r - l) / ((1u64 << 32) as f64))
    }

    #[inline(always)]
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        let n = v.len();
        for i in (1..n).rev() {
            let j = self.gen_range(0, i + 1);
            v.swap(i, j);
        }
    }
}

pub trait RandomSampler<T> {
    fn sample(&mut self) -> T;
}

pub struct DiscreteSampler<T> {
    buf: Vec<T>,
    rnd: Rnd,
}

impl<T: Copy> DiscreteSampler<T> {
    pub fn new(weight_values: &Vec<(usize, T)>) -> Self {
        let weight_sum = weight_values.iter().map(|(w, _)| *w).sum::<usize>();
        assert!(0 < weight_sum);
        assert!(weight_sum < 1_000_000);
        let mut buf = Vec::with_capacity(weight_sum);
        for &(w, val) in weight_values.iter() {
            buf.extend(std::iter::repeat(val).take(w));
        }
        Self {
            buf,
            rnd: Rnd::new(24),
        }
    }
}

impl<T: Copy> RandomSampler<T> for DiscreteSampler<T> {
    fn sample(&mut self) -> T {
        self.rnd.choice(&self.buf)
    }
}

pub struct ContinousSampler {
    buf: Vec<f64>,
    rnd: Rnd,
}

impl ContinousSampler {
    pub fn new<F>(f: F, x_min: f64, x_max: f64, size: usize) -> Self
    where
        F: Fn(f64) -> f64,
    {
        assert!(0 < size);
        assert!(size < 1_000_000);
        let mut buf = Vec::with_capacity(size);
        let step = (x_max - x_min) / (size as f64 - 1.);
        for i in 0..size {
            let x = x_min + step * (i as f64);
            buf.push(f(x));
        }
        Self {
            buf,
            rnd: Rnd::new(24),
        }
    }
}

impl RandomSampler<f64> for ContinousSampler {
    fn sample(&mut self) -> f64 {
        self.rnd.choice(&self.buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dicrete_sampler() {
        let weight_values = vec![(1, 'a'), (3, 'b'), (6, 'c')];
        let mut sampler = DiscreteSampler::new(&weight_values);
        let mut counts = std::collections::HashMap::new();
        for _ in 0..10000 {
            let v = sampler.sample();
            *counts.entry(v).or_insert(0) += 1;
        }
        assert!(counts[&'a'] < counts[&'b']);
        assert!(counts[&'b'] < counts[&'c']);
    }

    #[test]
    fn test_continous_sampler() {
        let f = |x: f64| x * x;
        let mut sampler = ContinousSampler::new(f, 0., 1., 100);
        let mut sum = 0.;
        for _ in 0..10000 {
            let v = sampler.sample();
            sum += v;
        }
        let mean = sum / 10000.;
        assert!((mean - 0.333).abs() < 0.01);
    }
}
