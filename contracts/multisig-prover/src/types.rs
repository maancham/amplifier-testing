use std::fmt::Display;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{from_binary, HexBinary, StdResult, Uint256};
use cw_storage_plus::{Key, KeyDeserialize, PrimaryKey};
use multisig::types::Signature;
use sha3::{Digest, Keccak256};

use crate::encoding::Data;

#[cw_serde]
pub enum CommandType {
    ApproveContractCall,
}

impl Display for CommandType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandType::ApproveContractCall => write!(f, "approveContractCall"),
        }
    }
}

#[cw_serde]
pub struct Command {
    pub id: HexBinary,
    pub ty: CommandType,
    pub params: HexBinary,
}

#[cw_serde]
pub struct BatchID(HexBinary);

impl From<HexBinary> for BatchID {
    fn from(id: HexBinary) -> Self {
        Self(id)
    }
}

impl From<&[u8]> for BatchID {
    fn from(id: &[u8]) -> Self {
        Self(id.into())
    }
}

impl<'a> PrimaryKey<'a> for BatchID {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = BatchID;
    type SuperSuffix = BatchID;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.0.as_slice())]
    }
}

impl KeyDeserialize for BatchID {
    type Output = BatchID;

    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        Ok(from_binary(&value.into()).expect("violated invariant: BatchID is not deserializable"))
    }
}

impl BatchID {
    pub fn new(message_ids: &[String]) -> BatchID {
        let mut message_ids = message_ids.to_vec();
        message_ids.sort();

        Keccak256::digest(message_ids.join(",")).as_slice().into()
    }
}

#[cw_serde]
pub struct CommandBatch {
    pub id: BatchID,
    pub message_ids: Vec<String>,
    pub data: Data,
}

#[cw_serde]
#[derive(Ord, PartialOrd, Eq)]
pub struct Operator {
    pub address: HexBinary,
    pub weight: Uint256,
    pub signature: Option<Signature>,
}