use crate::games::types::{CoinChoice, GameType, VRFBundle};
use schnorrkel::{Keypair, PublicKey, SecretKey, Signature};
use sha2::{Digest, Sha256};
use std::sync::Arc;

const VRF_SIGNING_CONTEXT: &[u8] = b"substrate";

/// VRF-based game outcome generator
pub struct VRFGameEngine {
    keypair: Arc<Keypair>,
}

impl VRFGameEngine {
    /// Create a new VRF engine with a keypair
    pub fn new(keypair: Keypair) -> Self {
        Self {
            keypair: Arc::new(keypair),
        }
    }

    /// Create a new VRF engine with a random keypair (for testing)
    pub fn new_random() -> Self {
        use rand_core::OsRng;
        let keypair = Keypair::generate_with(OsRng);
        Self::new(keypair)
    }

    /// Generate a provably fair game outcome using VRF
    pub fn generate_outcome(
        &self,
        game_id: &str,
        game_type: GameType,
        player_id: &str,
        additional_data: &str,
    ) -> Result<VRFBundle, String> {
        // Create deterministic input message
        let input_message = format!(
            "{}:{}:{}:{}",
            game_id, game_type, player_id, additional_data
        );

        // Generate VRF output and proof
        let (vrf_output, vrf_proof) = self.vrf_sign(input_message.as_bytes())?;

        // Get public key
        let public_key = self.keypair.public.to_bytes();

        Ok(VRFBundle {
            vrf_output: hex::encode(vrf_output),
            vrf_proof: hex::encode(vrf_proof),
            public_key: hex::encode(public_key),
            input_message,
        })
    }

    /// Internal VRF signing (generates output + proof)
    fn vrf_sign(&self, message: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
        use schnorrkel::context::SigningContext;

        // Create signing context
        let ctx = SigningContext::new(VRF_SIGNING_CONTEXT);
        
        // Sign the message (VRF output is derived from signature)
        let transcript = ctx.bytes(message);
        let signature = self.keypair.sign(transcript);

        // VRF output is hash of signature (deterministic)
        let mut hasher = Sha256::new();
        hasher.update(&signature.to_bytes());
        let vrf_output = hasher.finalize().to_vec();

        // VRF proof is the signature itself
        let vrf_proof = signature.to_bytes().to_vec();

        Ok((vrf_output, vrf_proof))
    }

    /// Compute coin flip result from VRF output
    pub fn compute_coinflip(vrf_output: &[u8]) -> CoinChoice {
        // Take the first byte and check if it's even or odd
        // (kept consistent with the API/game verification rule)
        let first_byte = vrf_output.first().copied().unwrap_or(0);
        if first_byte % 2 == 0 {
            CoinChoice::Heads
        } else {
            CoinChoice::Tails
        }
    }

    /// Verify a VRF proof (public verification function)
    pub fn verify_vrf_proof(
        vrf_bundle: &VRFBundle,
        expected_input: &str,
    ) -> Result<bool, String> {
        // Verify input message matches
        if vrf_bundle.input_message != expected_input {
            return Ok(false);
        }

        // Decode components
        let vrf_output = hex::decode(&vrf_bundle.vrf_output)
            .map_err(|e| format!("Invalid VRF output hex: {}", e))?;
        let vrf_proof = hex::decode(&vrf_bundle.vrf_proof)
            .map_err(|e| format!("Invalid VRF proof hex: {}", e))?;
        let public_key_bytes = hex::decode(&vrf_bundle.public_key)
            .map_err(|e| format!("Invalid public key hex: {}", e))?;

        // Parse public key
        let public_key_array: [u8; 32] = public_key_bytes
            .try_into()
            .map_err(|_| "Public key must be 32 bytes")?;
        let public_key = PublicKey::from_bytes(&public_key_array)
            .map_err(|e| format!("Invalid public key: {:?}", e))?;

        // Parse signature (VRF proof)
        let signature_array: [u8; 64] = vrf_proof
            .try_into()
            .map_err(|_| "Signature must be 64 bytes")?;
        let signature = Signature::from_bytes(&signature_array)
            .map_err(|e| format!("Invalid signature: {:?}", e))?;

        // Verify signature
        use schnorrkel::context::SigningContext;
        let ctx = SigningContext::new(VRF_SIGNING_CONTEXT);
        let transcript = ctx.bytes(expected_input.as_bytes());
        
        let is_valid = public_key.verify(transcript, &signature).is_ok();
        if !is_valid {
            return Ok(false);
        }

        // Verify VRF output is correctly derived from signature
        let mut hasher = Sha256::new();
        hasher.update(&signature_array);
        let computed_output = hasher.finalize();

        Ok(computed_output.as_slice() == vrf_output.as_slice())
    }

    /// Get the public key for this VRF engine
    pub fn public_key(&self) -> Vec<u8> {
        self.keypair.public.to_bytes().to_vec()
    }

    /// Get the public key as hex string
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.public_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vrf_generation_and_verification() {
        let engine = VRFGameEngine::new_random();
        
        let game_id = "test-game-123";
        let game_type = GameType::CoinFlip;
        let player_id = "player-456";
        let additional_data = "heads";

        // Generate VRF proof
        let vrf_bundle = engine
            .generate_outcome(game_id, game_type, player_id, additional_data)
            .expect("VRF generation failed");

        // Verify proof
        let expected_input = format!(
            "{}:{}:{}:{}",
            game_id, game_type, player_id, additional_data
        );
        let is_valid = VRFGameEngine::verify_vrf_proof(&vrf_bundle, &expected_input)
            .expect("Verification failed");

        assert!(is_valid, "VRF proof should be valid");
    }

    #[test]
    fn test_coinflip_deterministic() {
        let output1 = vec![0, 1, 1, 1]; // Even first byte
        let output2 = vec![1, 0, 0, 0]; // Odd first byte

        assert_eq!(VRFGameEngine::compute_coinflip(&output1), CoinChoice::Heads);
        assert_eq!(VRFGameEngine::compute_coinflip(&output2), CoinChoice::Tails);
    }

    #[test]
    fn test_vrf_tamper_detection() {
        let engine = VRFGameEngine::new_random();
        
        let mut vrf_bundle = engine
            .generate_outcome("game-1", GameType::CoinFlip, "player-1", "test")
            .expect("VRF generation failed");

        // Tamper with VRF output
        vrf_bundle.vrf_output = hex::encode(vec![0xff; 32]);

        // Verification should fail
        let is_valid = VRFGameEngine::verify_vrf_proof(
            &vrf_bundle,
            "game-1:coinflip:player-1:test",
        )
        .expect("Verification failed");

        assert!(!is_valid, "Tampered VRF should be invalid");
    }
}
