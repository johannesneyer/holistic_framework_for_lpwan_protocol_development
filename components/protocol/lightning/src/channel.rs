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

use rand_core::RngCore;

use crate::{Channel, NUM_CHANNELS};

#[derive(Debug, Default)]
pub(crate) struct Channels {
    pub(crate) public: Channel,
    pub(crate) parent: Option<Channel>,
    pub(crate) children: Option<Channel>,
    pub(crate) parents_parent_channel: Option<Channel>,
}

impl Channels {
    pub(crate) fn set_random_children_channel(&mut self, mut rng: impl RngCore) {
        let mut free_channels = (0..NUM_CHANNELS).filter(|c| {
            ![Some(self.public), self.parent, self.parents_parent_channel].contains(&Some(*c))
        });
        let num_free_channels = free_channels.clone().count();
        self.children = free_channels.nth(rng.next_u32() as usize % num_free_channels);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng as Rng;

    #[test]
    fn random_child_channel() {
        assert_eq!(NUM_CHANNELS, 8);
        let mut chs = Channels::default();
        chs.parent = Some(2);
        chs.parents_parent_channel = Some(4);
        for _ in 0..100 {
            chs.set_random_children_channel(Rng::default());
            assert!([1, 3, 5, 6, 7].contains(&chs.children.unwrap()));
        }
    }
}
