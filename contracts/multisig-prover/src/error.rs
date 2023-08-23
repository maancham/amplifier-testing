use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("")]
    Std,

    #[error("caller is not authorized")]
    Unauthorized,

    #[error("message is invalid")]
    InvalidMessage,

    #[error("public key is invalid")]
    InvalidPublicKey,

    #[error("chain name is invalid")]
    InvalidChainName,

    #[error("invalid participants")]
    InvalidParticipants,

    #[error("no messages found")]
    NoMessagesFound,

    #[error("no data in reply message")]
    NoDataInReply,

    #[error("wrong chain")]
    WrongChain,
}
