use crate::error::Error;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{HexBinary, Uint256};
use error_stack::Report;
use router_api::Address;

#[cw_serde]
#[derive(Eq)]
pub struct TokenId(
    #[serde(with = "axelar_wasm_std::hex")]
    #[schemars(with = "String")]
    [u8; 32],
);

#[cw_serde]
#[derive(Eq, Copy)]
#[repr(u8)]
pub enum TokenManagerType {
    NativeInterchainToken = 0,
    MintBurnFrom = 1,
    LockUnlock = 2,
    LockUnlockFee = 3,
    MintBurn = 4,
    Gateway = 5,
}

#[cw_serde]
#[derive(Eq)]
pub enum ITSMessage {
    InterchainTransfer {
        token_id: TokenId,
        source_address: HexBinary,
        destination_address: HexBinary,
        amount: Uint256,
        data: HexBinary,
    },
    DeployInterchainToken {
        token_id: TokenId,
        name: String,
        symbol: String,
        decimals: u8,
        minter: HexBinary,
    },
    DeployTokenManager {
        token_id: TokenId,
        token_manager_type: TokenManagerType,
        params: HexBinary,
    },
}

#[cw_serde]
#[derive(Eq)]
pub struct ITSRoutedMessage {
    /// Remote chain name.
    /// ITS edge source contract -> ITS Hub GMP call: Set to the true destination chain name.
    /// ITS Hub -> ITS edge destination contract: Set to the true source chain name.
    pub remote_chain: Address,
    pub message: ITSMessage,
}

impl TokenId {
    #[inline]
    pub const fn new(id: [u8; 32]) -> Self {
        Self(id)
    }

    #[inline]
    pub const fn to_bytes(&self) -> [u8; 32] {
        self.0
    }
}

impl TryFrom<u8> for TokenManagerType {
    type Error = Report<Error>;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => TokenManagerType::NativeInterchainToken,
            1 => TokenManagerType::MintBurnFrom,
            2 => TokenManagerType::LockUnlock,
            3 => TokenManagerType::LockUnlockFee,
            4 => TokenManagerType::MintBurn,
            5 => TokenManagerType::Gateway,
            _ => return Err(Report::new(Error::InvalidEnum)),
        })
    }
}