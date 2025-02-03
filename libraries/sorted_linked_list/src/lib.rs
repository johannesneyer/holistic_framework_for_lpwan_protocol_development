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

/// Min priority queue based on a linked list.
/// similar to https://docs.rs/heapless/latest/heapless/sorted_linked_list/index.html
use std::fmt::Debug;

pub struct SortedLinkedList<T: Ord> {
    head: Option<Box<Node<T>>>,
    len: usize,
}

impl<T: Ord> SortedLinkedList<T> {
    pub fn new() -> Self {
        Self { head: None, len: 0 }
    }

    pub fn push(&mut self, element: T) {
        let mut new_node = Box::new(Node {
            element,
            next: None,
        });

        let mut current = &mut self.head;

        loop {
            if current
                .as_ref()
                .map(|node| node.element > new_node.element)
                .unwrap_or(true)
            {
                // insert node
                new_node.next = current.take();
                *current = Some(new_node);
                self.len += 1;
                return;
            } else {
                // move to next node
                let node = current.as_mut().unwrap();
                current = &mut node.next;
            }
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        self.head.take().map(|head| {
            self.len -= 1;
            self.head = head.next;
            head.element
        })
    }

    pub fn peek(&mut self) -> Option<&T> {
        self.head.as_ref().map(|h| &h.element)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> Iter<T> {
        Iter {
            next: self.head.as_deref(),
            remaining: self.len,
        }
    }

    /// Warning: modifying an elements such that it's ordering relative to the other elements
    /// causes the list to not me sorted anymore.
    pub fn iter_mut(&mut self) -> IterMut<T> {
        IterMut {
            next: self.head.as_deref_mut(),
            remaining: self.len,
        }
    }

    // // nicer code but borrow checker complains even though logic is sound
    // pub fn retain(&mut self, mut f: impl FnMut(&T) -> bool) {
    //     let mut current = &mut self.head;
    //     while let Some(node) = current.as_mut() {
    //         if f(&node.element) {
    //             // move to next node
    //             current = &mut node.next;
    //         } else {
    //             // delete node
    //             self.len -= 1;
    //             *current = node.next.take();
    //         }
    //     }
    // }

    pub fn retain(&mut self, mut f: impl FnMut(&T) -> bool) {
        let mut current = &mut self.head;
        while current.is_some() {
            if current
                .as_ref()
                .map(|node| f(&node.element))
                .unwrap_or(false)
            {
                // move to next node
                let node = current.as_mut().unwrap();
                current = &mut node.next;
            } else {
                // delete node
                let node = current.as_mut().unwrap();
                *current = node.next.take();
                self.len -= 1;
            }
        }
    }

    pub fn as_vec(&self) -> Vec<&T> {
        Vec::from_iter(self.iter())
    }
}

impl<T: Ord> Default for SortedLinkedList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord + Debug> Debug for SortedLinkedList<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

pub struct Iter<'a, T> {
    next: Option<&'a Node<T>>,
    remaining: usize,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.next.take().map(|node| {
            self.next = node.next.as_deref();
            &node.element
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.remaining
    }
}

pub struct IterMut<'a, T> {
    next: Option<&'a mut Node<T>>,
    remaining: usize,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        self.next.take().map(|node| {
            self.next = node.next.as_deref_mut();
            &mut node.element
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }

    fn count(self) -> usize
    where
        Self: Sized,
    {
        self.remaining
    }
}

#[derive(Debug)]
struct Node<T> {
    element: T,
    next: Option<Box<Node<T>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let mut l = SortedLinkedList::new();
        assert_eq!(l.len(), 0);
        assert_eq!(l.pop(), None);
        l.push(1);
        assert_eq!(l.len(), 1);
        l.push(2);
        assert_eq!(l.len(), 2);

        assert_eq!(l.iter().count(), 2);
        assert_eq!(l.iter().size_hint(), (2, Some(2)));
        assert_eq!(l.iter_mut().count(), 2);
        assert_eq!(l.iter_mut().size_hint(), (2, Some(2)));

        assert_eq!(l.pop(), Some(1));
        assert_eq!(l.len(), 1);
        assert_eq!(l.pop(), Some(2));
        assert_eq!(l.len(), 0);
    }

    #[test]
    fn is_sorted() {
        let mut l = SortedLinkedList::new();
        l.push(4);
        l.push(3);
        l.push(4);
        l.push(2);
        l.push(1);
        l.push(5);
        l.push(0);
        l.push(0);
        assert_eq!(l.as_vec(), [&0, &0, &1, &2, &3, &4, &4, &5])
    }

    #[test]
    fn retain() {
        let mut l = SortedLinkedList::new();
        l.push(3);
        l.push(1);
        l.push(4);
        l.push(2);
        l.push(1);
        l.push(0);
        l.push(5);
        l.push(0);
        l.retain(|v| *v > 2);
        assert_eq!(l.as_vec(), [&3, &4, &5]);
        l.retain(|v| *v > 10);
        assert_eq!(l.len, 0);
        assert_eq!(l.pop(), None);
    }
}
