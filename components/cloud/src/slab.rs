//  _____       ______   ____
// |_   _|     |  ____|/ ____|  Institute of Embedded Systems
//   | |  _ __ | |__  | (___    Zurich University of Applied Sciences
//   | | | '_ \|  __|  \___ \   8401 Winterthur, Switzerland
//  _| |_| | | | |____ ____) |
// |_____|_| |_|______|_____/
//
// Copyright 2025 Institute of Embedded Systems at Zurich University of Applied Sciences.
// All rights reserved.
// SPDX-License-Identifier: MIT

//! Collection similar to https://docs.rs/slab/latest/slab/

use std::slice;

pub struct Slab<T>(Vec<Option<T>>);

impl<T> Slab<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn insert(&mut self, element: T) -> usize {
        if let Some(index) = self.get_free_index() {
            self.0[index] = Some(element);
            index
        } else {
            self.0.push(Some(element));
            self.0.len() - 1
        }
    }

    /// Returns index of first free slot or None if all slots are occupied.
    fn get_free_index(&self) -> Option<usize> {
        for (index, slot) in self.0.iter().enumerate() {
            if slot.is_none() {
                return Some(index);
            }
        }
        None
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.0.get_mut(index)?.as_mut()
    }

    #[allow(dead_code)]
    pub fn get(&mut self, index: usize) -> Option<&T> {
        self.0.get(index)?.as_ref()
    }

    pub fn try_remove(&mut self, index: usize) -> Option<T> {
        if index < self.0.len() {
            self.0.get_mut(index)?.take()
        } else {
            None
        }
    }

    pub fn iter(&self) -> slice::Iter<'_, Option<T>> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> slice::IterMut<'_, Option<T>> {
        self.0.iter_mut()
    }
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> IntoIterator for &'a Slab<T> {
    type Item = &'a Option<T>;
    type IntoIter = slice::Iter<'a, Option<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Slab<T> {
    type Item = &'a mut Option<T>;
    type IntoIter = slice::IterMut<'a, Option<T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        let mut slab = Slab::new();

        let i1 = slab.insert(1);
        let i2 = slab.insert(2);

        assert_eq!(slab.get(i1), Some(1).as_ref());
        assert_eq!(slab.get_mut(i2), Some(2).as_mut());
    }
}
