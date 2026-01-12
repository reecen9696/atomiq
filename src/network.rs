//! Simple mock network for testing - can be replaced with real networking later

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use hotstuff_rs::{
    networking::{messages::Message, network::Network},
    types::{
        crypto_primitives::VerifyingKey,
        update_sets::ValidatorSetUpdates,
        validator_set::ValidatorSet,
    },
};

pub struct MockNetwork {
    _id: VerifyingKey,
    _peers: HashMap<VerifyingKey, Sender<(VerifyingKey, Message)>>,
    receiver: Receiver<(VerifyingKey, Message)>,
}

impl MockNetwork {
    pub fn new(id: VerifyingKey) -> Self {
        let (_, receiver) = channel();
        Self {
            _id: id,
            _peers: HashMap::new(),
            receiver,
        }
    }
}

impl Network for MockNetwork {
    fn init_validator_set(&mut self, _validator_set: ValidatorSet) {}

    fn update_validator_set(&mut self, _updates: ValidatorSetUpdates) {}

    fn broadcast(&mut self, _message: Message) {
        // No-op for single node testing
    }

    fn send(&mut self, _peer: VerifyingKey, _message: Message) {
        // No-op for single node testing
    }

    fn recv(&mut self) -> Option<(VerifyingKey, Message)> {
        self.receiver.try_recv().ok()
    }
}

impl Clone for MockNetwork {
    fn clone(&self) -> Self {
        let (_, receiver) = channel();
        Self {
            _id: self._id,
            _peers: HashMap::new(),
            receiver,
        }
    }
}