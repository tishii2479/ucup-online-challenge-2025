#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct IndexSet {
    pub que: Vec<usize>,
    pub pos: Vec<usize>,
}

impl IndexSet {
    pub fn empty(n: usize) -> Self {
        IndexSet {
            que: Vec::with_capacity(n),
            pos: vec![!0; n],
        }
    }

    pub fn full(n: usize) -> Self {
        IndexSet {
            que: (0..n).collect(),
            pos: (0..n).collect(),
        }
    }

    pub fn add(&mut self, v: usize) {
        if self.contains(v) {
            return;
        }
        self.pos[v] = self.que.len();
        self.que.push(v);
    }

    pub fn remove(&mut self, v: usize) {
        if !self.contains(v) {
            return;
        }

        let p = self.pos[v];
        let b = self.que[self.que.len() - 1];
        self.que.swap_remove(p);
        self.pos[b] = p;
        self.pos[v] = !0;
    }

    pub fn contains(&self, v: usize) -> bool {
        self.pos[v] != !0
    }

    pub fn size(&self) -> usize {
        self.que.len()
    }

    pub fn get_first(&self) -> Option<usize> {
        self.que.get(0).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = &usize> {
        self.que.iter()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Rnd {
    state: u32,
}

impl Rnd {
    pub fn new(mut seed: u32) -> Self {
        if seed == 0 {
            seed = u32::MAX;
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
