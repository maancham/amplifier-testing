use cosmwasm_std::{from_binary, DepsMut, Reply, Response};
use cw_utils::{parse_reply_execute_data, MsgExecuteContractResponse};

use error_stack::{IntoReport, Result, ResultExt};

use crate::{
    error::ContractError,
    events::Event,
    state::{PROOF_BATCH_MULTISIG, REPLY_BATCH},
    types::ProofID,
};

pub fn start_multisig_reply(deps: DepsMut, reply: Reply) -> Result<Response, ContractError> {
    match parse_reply_execute_data(reply) {
        Ok(MsgExecuteContractResponse { data: Some(data) }) => {
            let command_batch_id = REPLY_BATCH
                .load(deps.storage)
                .into_report()
                .change_context(ContractError::Std)?;

            let session_id = from_binary(&data)
                .into_report()
                .change_context(ContractError::Std)?;

            let proof_id = ProofID::new(&command_batch_id, &session_id);

            PROOF_BATCH_MULTISIG
                .save(deps.storage, &proof_id, &(command_batch_id, session_id))
                .into_report()
                .change_context(ContractError::Std)?;

            Ok(Response::new().add_event(Event::ProofUnderConstruction { proof_id }.into()))
        }
        Ok(MsgExecuteContractResponse { data: None }) => {
            Err(ContractError::NoDataInReply).into_report()
        }
        Err(_) => {
            unreachable!("violated invariant: replied failed submessage with ReplyOn::Success")
        }
    }
}
