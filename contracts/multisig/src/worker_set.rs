use std::collections::{BTreeSet, HashMap};

use crate::key::KeyType;
use crate::{key::PublicKey, ContractError};
use crate::msg::Signer;
use axelar_wasm_std::Participant;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{HexBinary, Uint256};
use sha3::{Digest, Keccak256};

#[cw_serde]
pub struct WorkerSet {
    pub signers: BTreeSet<Signer>,
    pub threshold: Uint256,
    // for hash uniqueness. The same exact worker set could be in use at two different times,
    // and we need to be able to distinguish between the two
    pub created_at: u64,
    // TODO: add nonce to the voting verifier and to the evm gateway.
    // Without a nonce, updating to a worker set that is the exact same as a worker set in the past will be immediately confirmed.
    // https://github.com/axelarnetwork/axelar-amplifier/pull/70#discussion_r1323454223
}

impl WorkerSet {
    pub fn new(
        participants: Vec<(Participant, PublicKey)>,
        threshold: Uint256,
        block_height: u64,
    ) -> Self {
        let signers = participants
            .into_iter()
            .map(|(participant, pub_key)| Signer {
                address: participant.address.clone(),
                weight: participant.weight.into(),
                pub_key,
            })
            .collect();

        WorkerSet {
            signers,
            threshold,
            created_at: block_height,
        }
    }

    pub fn hash(&self) -> HexBinary {
        Keccak256::digest(serde_json::to_vec(&self).expect("couldn't serialize worker set"))
            .as_slice()
            .into()
    }

    pub fn id(&self) -> String {
        self.hash().to_hex()
    }

    pub fn get_pub_keys_from_signer(&self) -> Result<HashMap<String, PublicKey>, ContractError>{
        let mut pub_keys = HashMap::new();
        for signer in self.signers {
            let public_key_result = PublicKey::try_from((KeyType::Ecdsa, signer.pub_key.as_ref().into()));
    
            match public_key_result {
                Ok(public_key) => {
                    pub_keys.insert(signer.address.to_string(), public_key);
                },
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(pub_keys)
    }
}
