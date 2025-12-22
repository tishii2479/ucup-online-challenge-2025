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
