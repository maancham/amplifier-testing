use std::str::FromStr;

use connection_router::types::ChainName;
use cosmwasm_std::{Addr, Coin, Empty, Fraction, Uint128};
use cw_multi_test::{next_block, App, AppBuilder, Contract, ContractWrapper, Executor};

use crate::mocks;

#[path = "../../src/test/test_data.rs"]
mod test_data;

pub const INSTANTIATOR: &str = "instantiator";
pub const RELAYER: &str = "relayer";

pub struct TestCaseConfig {
    pub app: App,
    pub admin: Addr,
    pub prover_address: Addr,
}

pub fn mock_app() -> App {
    AppBuilder::new().build(|router, _, storage| {
        router
            .bank
            .init_balance(
                storage,
                &Addr::unchecked(RELAYER),
                vec![Coin {
                    denom: "uaxl".to_string(),
                    amount: Uint128::from(100u8),
                }],
            )
            .unwrap();
    })
}

fn contract_multisig() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        mocks::multisig::execute,
        mocks::multisig::instantiate,
        mocks::multisig::query,
    );
    Box::new(contract)
}

fn instantiate_mock_multisig(app: &mut App) -> Addr {
    let code_id = app.store_code(contract_multisig());
    let msg = multisig::msg::InstantiateMsg {};

    app.instantiate_contract(
        code_id,
        Addr::unchecked(INSTANTIATOR),
        &msg,
        &[],
        "mock-multisig",
        None,
    )
    .unwrap()
}

fn contract_gateway() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        mocks::gateway::execute,
        mocks::gateway::instantiate,
        mocks::gateway::query,
    );
    Box::new(contract)
}

fn instantiate_mock_gateway(app: &mut App) -> Addr {
    let code_id = app.store_code(contract_gateway());
    let msg = gateway::msg::InstantiateMsg {
        verifier_address: "verifier".to_string(),
        router_address: "router".to_string(),
    };

    app.instantiate_contract(
        code_id,
        Addr::unchecked(INSTANTIATOR),
        &msg,
        &[],
        "mock-gateway",
        None,
    )
    .unwrap()
}

fn contract_service_registry() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        mocks::service_registry::execute,
        mocks::service_registry::instantiate,
        mocks::service_registry::query,
    );
    Box::new(contract)
}

fn instantiate_mock_service_registry(app: &mut App) -> Addr {
    let code_id = app.store_code(contract_service_registry());
    let msg = service_registry::msg::InstantiateMsg {
        governance_account: "governance".to_string(),
    };

    app.instantiate_contract(
        code_id,
        Addr::unchecked(INSTANTIATOR),
        &msg,
        &[],
        "mock-service-registry",
        None,
    )
    .unwrap()
}

fn contract_prover() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        command_batcher::contract::execute,
        command_batcher::contract::instantiate,
        command_batcher::contract::query,
    )
    .with_reply(command_batcher::contract::reply);
    Box::new(contract)
}

fn instantiate_prover(
    app: &mut App,
    gateway_address: String,
    multisig_address: String,
    service_registry_address: String,
) -> Addr {
    let code_id = app.store_code(contract_prover());
    let msg = command_batcher::msg::InstantiateMsg {
        gateway_address,
        multisig_address,
        service_registry_address,
        destination_chain_id: test_data::destination_chain_id(),
        signing_threshold: (
            test_data::threshold().numerator(),
            test_data::threshold().denominator(),
        ),
        service_name: "service-name".to_string(),
        chain_name: ChainName::from_str("Ethereum").unwrap(),
    };

    app.instantiate_contract(
        code_id,
        Addr::unchecked(INSTANTIATOR),
        &msg,
        &[],
        "prover",
        None,
    )
    .unwrap()
}

pub fn setup_test_case() -> TestCaseConfig {
    let mut app = mock_app();

    let gateway_address = instantiate_mock_gateway(&mut app);
    let multisig_address = instantiate_mock_multisig(&mut app);
    let service_registry_address = instantiate_mock_service_registry(&mut app);

    let prover_address = instantiate_prover(
        &mut app,
        gateway_address.to_string(),
        multisig_address.to_string(),
        service_registry_address.to_string(),
    );

    app.update_block(next_block);

    TestCaseConfig {
        app,
        admin: Addr::unchecked(INSTANTIATOR),
        prover_address,
    }
}