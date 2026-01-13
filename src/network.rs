//! Mock network for single validator testing
//! 
//! Can be replaced with real P2P networking for multi-validator setup

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, mpsc::Sender};
use hotstuff_rs::{
    networking::{messages::Message, network::Network},
    types::{
        crypto_primitives::VerifyingKey,
        update_sets::ValidatorSetUpdates,
        validator_set::ValidatorSet,
    },
};

/// Mock network implementation that supports single-validator consensus by allowing
/// the validator to send messages to itself
pub struct MockNetwork {
    id: VerifyingKey,
    peers: HashMap<VerifyingKey, Sender<(VerifyingKey, Message)>>,
    message_queue: Arc<Mutex<VecDeque<(VerifyingKey, Message)>>>,
}

impl MockNetwork {
    pub fn new(id: VerifyingKey) -> Self {
        Self {
            id,
            peers: HashMap::new(),
            message_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl Network for MockNetwork {
    fn init_validator_set(&mut self, _validator_set: ValidatorSet) {
        // No-op for single node - validator set is handled in replica initialization
    }

    fn update_validator_set(&mut self, _updates: ValidatorSetUpdates) {
        // No-op for single node
    }

    fn broadcast(&mut self, message: Message) {
        // In single validator setup, broadcast to self
        if let Ok(mut queue) = self.message_queue.lock() {
            queue.push_back((self.id, message));
        }
    }

    fn send(&mut self, peer: VerifyingKey, message: Message) {
        // For single validator, if sending to self, add to message queue
        if peer == self.id {
            if let Ok(mut queue) = self.message_queue.lock() {
                queue.push_back((peer, message));
            }
        }
        // Ignore messages to other peers (not relevant in single validator setup)
    }

    fn recv(&mut self) -> Option<(VerifyingKey, Message)> {
        if let Ok(mut queue) = self.message_queue.lock() {
            queue.pop_front()
        } else {
            None
        }
    }
}

impl Clone for MockNetwork {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            peers: HashMap::new(),
            message_queue: self.message_queue.clone(),
        }
    }
}