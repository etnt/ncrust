//! Allocation of `<rpc>` message ids.
//!
//! May depend on: nothing.

/// Hands out `<rpc>` message ids in sequence.
#[derive(Debug, Clone)]
pub struct MessageIds {
    next: u32,
}

impl MessageIds {
    pub fn starting_at(first: u32) -> Self {
        MessageIds { next: first }
    }

    pub fn next_id(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_sequential_from_the_start_value() {
        let mut ids = MessageIds::starting_at(2);
        assert_eq!((ids.next_id(), ids.next_id(), ids.next_id()), (2, 3, 4));
    }
}
