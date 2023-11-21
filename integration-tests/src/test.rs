#[cfg(test)]
mod test {

    use axelar_wasm_std::nonempty;
    use connection_router::state::{ChainName, CrossChainId, Message};
    use cosmwasm_std::{
        coins, Addr, Binary, BlockInfo, Deps, Env, HexBinary, StdResult, Uint128, Uint256, Uint64,
    };
    use cw_multi_test::{App, ContractWrapper, Executor};

    use k256::ecdsa;
    use multisig::key::PublicKey;
    use tofn::ecdsa::KeyPair;

    const AXL_DENOMINATION: &str = "uaxl";
    #[test]
    fn test_basic_message_flow() {
        let mut protocol = setup_protocol("validators".to_string().try_into().unwrap());
        let chains = vec![
            "Ethereum".to_string().try_into().unwrap(),
            "Polygon".to_string().try_into().unwrap(),
        ];
        let workers = vec![
            Worker {
                addr: Addr::unchecked("worker1"),
                supported_chains: chains.clone(),
                key_pair: generate_key(0),
            },
            Worker {
                addr: Addr::unchecked("worker2"),
                supported_chains: chains.clone(),
                key_pair: generate_key(1),
            },
        ];
        register_workers(
            &mut protocol.app,
            protocol.service_registry_address.clone(),
            protocol.multisig_address.clone(),
            protocol.service_name.clone(),
            protocol.governance_address.clone(),
            &workers,
            protocol.genesis.clone(),
        );
        let chain1 = setup_chain(
            &mut protocol.app,
            protocol.router_address.clone(),
            protocol.service_registry_address.clone(),
            protocol.rewards_address.clone(),
            protocol.multisig_address.clone(),
            protocol.governance_address.clone(),
            protocol.genesis.clone(),
            protocol.service_name.clone(),
            chains.get(0).unwrap().clone(),
        );
        let chain2 = setup_chain(
            &mut protocol.app,
            protocol.router_address.clone(),
            protocol.service_registry_address.clone(),
            protocol.rewards_address.clone(),
            protocol.multisig_address.clone(),
            protocol.governance_address.clone(),
            protocol.genesis.clone(),
            protocol.service_name.clone(),
            chains.get(1).unwrap().clone(),
        );

        let msg = Message {
            cc_id: CrossChainId {
                chain: chain1.chain_name.clone(),
                id: "0x88d7956fd7b6fcec846548d83bd25727f2585b4be3add21438ae9fbb34625924:3"
                    .to_string()
                    .try_into()
                    .unwrap(),
            },
            source_address: "0xBf12773B49()0e1Deb57039061AAcFA2A87DEaC9b9"
                .to_string()
                .try_into()
                .unwrap(),
            destination_address: "0xce16F69375520ab01377ce7B88f5BA8C48F8D666"
                .to_string()
                .try_into()
                .unwrap(),
            destination_chain: chain2.chain_name,
            payload_hash: HexBinary::from_hex(
                "3e50a012285f8e7ec59b558179cd546c55c477ebe16202aac7d7747e25be03be",
            )
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap(),
        };
        let relayer = Addr::unchecked("relayer");

        protocol
            .app
            .execute_contract(
                relayer.clone(),
                chain1.gateway_address.clone(),
                &gateway::msg::ExecuteMsg::VerifyMessages(vec![msg.clone()]),
                &[],
            )
            .unwrap();

        for worker in &workers {
            protocol
                .app
                .execute_contract(
                    worker.addr.clone(),
                    chain1.voting_verifier_address.clone(),
                    &voting_verifier::msg::ExecuteMsg::Vote {
                        poll_id: Uint64::one().into(),
                        votes: vec![true],
                    },
                    &[],
                )
                .unwrap();
        }

        protocol
            .app
            .execute_contract(
                relayer.clone(),
                chain1.voting_verifier_address.clone(),
                &voting_verifier::msg::ExecuteMsg::EndPoll {
                    poll_id: Uint64::one().into(),
                },
                &[],
            )
            .unwrap();

        protocol
            .app
            .execute_contract(
                relayer.clone(),
                chain1.gateway_address.clone(),
                &gateway::msg::ExecuteMsg::RouteMessages(vec![msg.clone()]),
                &[],
            )
            .unwrap();

        let messages: Vec<Message> = protocol
            .app
            .wrap()
            .query_wasm_smart(
                chain2.gateway_address,
                &gateway::msg::QueryMsg::GetMessages {
                    message_ids: vec![msg.cc_id.clone()],
                },
            )
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages.get(0), Some(&msg));

        let res = protocol
            .app
            .execute_contract(
                relayer.clone(),
                chain2.multisig_prover_address.clone(),
                &multisig_prover::msg::ExecuteMsg::ConstructProof {
                    message_ids: vec![msg.cc_id.to_string()],
                },
                &[],
            )
            .unwrap();

        let mut msg_to_sign = "".to_string();
        for event in res.events {
            let attr = event.attributes.iter().find(|attr| attr.key == "msg");
            if attr.is_some() {
                msg_to_sign = attr.clone().unwrap().value.clone();
            }
        }
        assert!(msg_to_sign != "");
        for worker in &workers {
            let signature = tofn::ecdsa::sign(
                worker.key_pair.signing_key(),
                &HexBinary::from_hex(&msg_to_sign)
                    .unwrap()
                    .as_slice()
                    .try_into()
                    .unwrap(),
            )
            .unwrap();

            let sig = ecdsa::Signature::from_der(&signature).unwrap();

            protocol
                .app
                .execute_contract(
                    worker.addr.clone(),
                    protocol.multisig_address.clone(),
                    &multisig::msg::ExecuteMsg::SubmitSignature {
                        session_id: Uint64::one(),
                        signature: HexBinary::from(sig.to_vec()),
                    },
                    &[],
                )
                .unwrap();
        }

        let proof_response: multisig_prover::msg::GetProofResponse = protocol
            .app
            .wrap()
            .query_wasm_smart(
                &chain2.multisig_prover_address,
                &multisig_prover::msg::QueryMsg::GetProof {
                    multisig_session_id: Uint64::one(),
                },
            )
            .unwrap();
        assert!(matches!(
            proof_response.status,
            multisig_prover::msg::ProofStatus::Completed { execute_data }
        ));
        assert_eq!(proof_response.message_ids, vec![msg.cc_id.to_string()]);

        let old_block = protocol.app.block_info();
        protocol.app.set_block(BlockInfo {
            height: old_block.height + 20,
            ..old_block
        });

        let res = protocol
            .app
            .execute_contract(
                relayer.clone(),
                protocol.rewards_address.clone(),
                &rewards::msg::ExecuteMsg::DistributeRewards {
                    contract_address: chain1.voting_verifier_address.to_string(),
                    epoch_count: None,
                },
                &[],
            )
            .unwrap();

        println!("{:?}", res);
        for worker in &workers {
            let balance = protocol
                .app
                .wrap()
                .query_balance(&worker.addr, AXL_DENOMINATION)
                .unwrap();
            assert_eq!(balance.amount, Uint128::from(50u128));
        }

        let res = protocol
            .app
            .execute_contract(
                relayer,
                protocol.rewards_address,
                &rewards::msg::ExecuteMsg::DistributeRewards {
                    contract_address: protocol.multisig_address.to_string(),
                    epoch_count: None,
                },
                &[],
            )
            .unwrap();
        println!("{:?}", res);
        for worker in workers {
            let balance = protocol
                .app
                .wrap()
                .query_balance(worker.addr, AXL_DENOMINATION)
                .unwrap();
            assert_eq!(balance.amount, Uint128::from(100u128));
        }
    }

    #[allow(dead_code)]
    struct Protocol {
        genesis: Addr, // holds u128::max coins, can use to send coins to other addresses
        governance_address: Addr,
        router_address: Addr,
        router_admin_address: Addr,
        multisig_address: Addr,
        service_registry_address: Addr,
        service_name: nonempty::String,
        rewards_address: Addr,
        app: App,
    }

    fn setup_protocol(service_name: nonempty::String) -> Protocol {
        let genesis = Addr::unchecked("genesis");
        let mut app = App::new(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &genesis, coins(u128::MAX, AXL_DENOMINATION))
                .unwrap()
        });
        let router_admin_address = Addr::unchecked("admin");
        let governance_address = Addr::unchecked("governance");

        let router_address = instantiate_connection_router(
            &mut app,
            connection_router::msg::InstantiateMsg {
                admin_address: router_admin_address.to_string(),
                governance_address: governance_address.to_string(),
            },
        );
        let rewards_address = instantiate_rewards(
            &mut app,
            rewards::msg::InstantiateMsg {
                governance_address: governance_address.to_string(),
                rewards_denom: AXL_DENOMINATION.to_string(),
                params: rewards::msg::Params {
                    epoch_duration: nonempty::Uint64::try_from(10u64).unwrap(),
                    rewards_per_epoch: Uint128::from(100u128).try_into().unwrap(),
                    participation_threshold: (1, 2).try_into().unwrap(),
                },
            },
        );
        let multisig_address = instantiate_multisig(
            &mut app,
            multisig::msg::InstantiateMsg {
                rewards_address: rewards_address.to_string(),
                governance_address: governance_address.to_string(),
                grace_period: 2,
            },
        );
        let service_registry_address = instantiate_service_registry(
            &mut app,
            service_registry::msg::InstantiateMsg {
                governance_account: governance_address.to_string(),
            },
        );
        app.execute_contract(
            genesis.clone(),
            rewards_address.clone(),
            &rewards::msg::ExecuteMsg::AddRewards {
                contract_address: multisig_address.to_string(),
            },
            &coins(1000, AXL_DENOMINATION),
        )
        .unwrap();

        Protocol {
            genesis,
            governance_address,
            router_address,
            router_admin_address,
            multisig_address,
            service_registry_address,
            service_name,
            rewards_address,
            app,
        }
    }

    // return the all-zero array with the first bytes set to the bytes of `index`
    fn generate_key(seed: u32) -> KeyPair {
        let index_bytes = seed.to_be_bytes();
        let mut result = [0; 64];
        result[0..index_bytes.len()].copy_from_slice(index_bytes.as_slice());
        let secret_recovery_key = result.as_slice().try_into().unwrap();
        tofn::ecdsa::keygen(&secret_recovery_key, b"tofn nonce").unwrap()
    }

    struct Worker {
        addr: Addr,
        supported_chains: Vec<ChainName>,
        key_pair: KeyPair,
    }

    fn register_workers(
        app: &mut App,
        service_registry: Addr,
        multisig: Addr,
        service_name: nonempty::String,
        governance_addr: Addr,
        workers: &Vec<Worker>,
        genesis: Addr,
    ) {
        let min_worker_bond = Uint128::new(100);
        app.execute_contract(
            governance_addr.clone(),
            service_registry.clone(),
            &service_registry::msg::ExecuteMsg::RegisterService {
                service_name: service_name.to_string(),
                service_contract: Addr::unchecked("nowhere"),
                min_num_workers: 0,
                max_num_workers: Some(100),
                min_worker_bond,
                bond_denom: AXL_DENOMINATION.into(),
                unbonding_period_days: 10,
                description: "Some service".into(),
            },
            &[],
        )
        .unwrap();

        app.execute_contract(
            governance_addr,
            service_registry.clone(),
            &service_registry::msg::ExecuteMsg::AuthorizeWorkers {
                workers: workers
                    .iter()
                    .map(|worker| worker.addr.to_string())
                    .collect(),
                service_name: service_name.to_string(),
            },
            &[],
        )
        .unwrap();

        for worker in workers {
            app.send_tokens(
                genesis.clone(),
                worker.addr.clone(),
                &coins(min_worker_bond.u128(), AXL_DENOMINATION),
            )
            .unwrap();
            app.execute_contract(
                worker.addr.clone(),
                service_registry.clone(),
                &service_registry::msg::ExecuteMsg::BondWorker {
                    service_name: service_name.to_string(),
                },
                &coins(min_worker_bond.u128(), AXL_DENOMINATION),
            )
            .unwrap();

            app.execute_contract(
                worker.addr.clone(),
                service_registry.clone(),
                &service_registry::msg::ExecuteMsg::DeclareChainSupport {
                    service_name: service_name.to_string(),
                    chains: worker.supported_chains.clone(),
                },
                &[],
            )
            .unwrap();

            app.execute_contract(
                worker.addr.clone(),
                multisig.clone(),
                &multisig::msg::ExecuteMsg::RegisterPublicKey {
                    public_key: PublicKey::Ecdsa(HexBinary::from(
                        worker.key_pair.encoded_verifying_key(),
                    )),
                },
                &[],
            )
            .unwrap();
        }
    }

    #[allow(dead_code)]
    #[derive(Clone)]
    struct Chain {
        gateway_address: Addr,
        voting_verifier_address: Addr,
        multisig_prover_address: Addr,
        chain_name: ChainName,
    }

    fn setup_chain(
        mut app: &mut App,
        router_address: Addr,
        service_registry_address: Addr,
        rewards_address: Addr,
        multisig_address: Addr,
        governance_address: Addr,
        genesis_address: Addr,
        service_name: nonempty::String,
        chain_name: ChainName,
    ) -> Chain {
        let voting_verifier_address = instantiate_voting_verifier(
            &mut app,
            voting_verifier::msg::InstantiateMsg {
                service_registry_address: service_registry_address.to_string().try_into().unwrap(),
                service_name: service_name.clone(),
                source_gateway_address: "doesn't matter".to_string().try_into().unwrap(),
                voting_threshold: (9, 10).try_into().unwrap(),
                block_expiry: 10,
                confirmation_height: 5,
                source_chain: chain_name.clone(),
                rewards_address: rewards_address.to_string(),
            },
        );
        let gateway_address = instantiate_gateway(
            &mut app,
            gateway::msg::InstantiateMsg {
                router_address: router_address.to_string(),
                verifier_address: voting_verifier_address.to_string(),
            },
        );
        let multisig_prover_address = instantiate_multisig_prover(
            &mut app,
            multisig_prover::msg::InstantiateMsg {
                admin_address: Addr::unchecked("doesn't matter").to_string(),
                gateway_address: gateway_address.to_string(),
                multisig_address: multisig_address.to_string(),
                service_registry_address: service_registry_address.to_string(),
                voting_verifier_address: voting_verifier_address.to_string(),
                destination_chain_id: Uint256::zero(),
                signing_threshold: (2, 3).try_into().unwrap(),
                service_name: service_name.to_string(),
                chain_name: chain_name.to_string(),
                worker_set_diff_threshold: 1,
                encoder: multisig_prover::encoding::Encoder::Abi,
                key_type: multisig::key::KeyType::Ecdsa,
            },
        );
        app.execute_contract(
            Addr::unchecked("doesn't matter"),
            multisig_prover_address.clone(),
            &multisig_prover::msg::ExecuteMsg::UpdateWorkerSet,
            &[],
        )
        .unwrap();
        app.execute_contract(
            governance_address.clone(),
            multisig_address,
            &multisig::msg::ExecuteMsg::AuthorizeCaller {
                contract_address: multisig_prover_address.clone(),
            },
            &[],
        )
        .unwrap();

        app.execute_contract(
            governance_address,
            router_address,
            &connection_router::msg::ExecuteMsg::RegisterChain {
                chain: chain_name.clone(),
                gateway_address: gateway_address.to_string(),
            },
            &[],
        )
        .unwrap();

        app.execute_contract(
            genesis_address.clone(),
            rewards_address.clone(),
            &rewards::msg::ExecuteMsg::AddRewards {
                contract_address: voting_verifier_address.to_string(),
            },
            &coins(1000, AXL_DENOMINATION),
        )
        .unwrap();

        Chain {
            gateway_address,
            voting_verifier_address,
            multisig_prover_address,
            chain_name,
        }
    }

    fn instantiate_connection_router(
        app: &mut App,
        instantiate_msg: connection_router::msg::InstantiateMsg,
    ) -> Addr {
        let code = ContractWrapper::new(
            connection_router::contract::execute,
            connection_router::contract::instantiate,
            connection_router::contract::query,
        );
        let code_id = app.store_code(Box::new(code));

        app.instantiate_contract(
            code_id,
            Addr::unchecked("anyone"),
            &instantiate_msg,
            &[],
            "connection_router",
            None,
        )
        .unwrap()
    }

    fn instantiate_multisig(app: &mut App, instantiate_msg: multisig::msg::InstantiateMsg) -> Addr {
        let code = ContractWrapper::new(
            multisig::contract::execute,
            multisig::contract::instantiate,
            multisig::contract::query,
        );
        let code_id = app.store_code(Box::new(code));

        app.instantiate_contract(
            code_id,
            Addr::unchecked("anyone"),
            &instantiate_msg,
            &[],
            "multisig",
            None,
        )
        .unwrap()
    }

    fn instantiate_rewards(app: &mut App, instantiate_msg: rewards::msg::InstantiateMsg) -> Addr {
        let code = ContractWrapper::new(
            rewards::contract::execute,
            rewards::contract::instantiate,
            |_: Deps, _: Env, _: rewards::msg::QueryMsg| -> StdResult<Binary> { todo!() },
        );
        let code_id = app.store_code(Box::new(code));

        app.instantiate_contract(
            code_id,
            Addr::unchecked("anyone"),
            &instantiate_msg,
            &[],
            "rewards",
            None,
        )
        .unwrap()
    }

    fn instantiate_voting_verifier(
        app: &mut App,
        instantiate_msg: voting_verifier::msg::InstantiateMsg,
    ) -> Addr {
        let code = ContractWrapper::new(
            voting_verifier::contract::execute,
            voting_verifier::contract::instantiate,
            voting_verifier::contract::query,
        );
        let code_id = app.store_code(Box::new(code));

        app.instantiate_contract(
            code_id,
            Addr::unchecked("anyone"),
            &instantiate_msg,
            &[],
            "voting_verifier",
            None,
        )
        .unwrap()
    }

    fn instantiate_gateway(app: &mut App, instantiate_msg: gateway::msg::InstantiateMsg) -> Addr {
        let code = ContractWrapper::new(
            gateway::contract::execute,
            gateway::contract::instantiate,
            gateway::contract::query,
        );
        let code_id = app.store_code(Box::new(code));

        app.instantiate_contract(
            code_id,
            Addr::unchecked("anyone"),
            &instantiate_msg,
            &[],
            "gateway",
            None,
        )
        .unwrap()
    }

    fn instantiate_service_registry(
        app: &mut App,
        instantiate_msg: service_registry::msg::InstantiateMsg,
    ) -> Addr {
        let code = ContractWrapper::new(
            service_registry::contract::execute,
            service_registry::contract::instantiate,
            service_registry::contract::query,
        );
        let code_id = app.store_code(Box::new(code));

        app.instantiate_contract(
            code_id,
            Addr::unchecked("anyone"),
            &instantiate_msg,
            &[],
            "service_registry",
            None,
        )
        .unwrap()
    }

    fn instantiate_multisig_prover(
        app: &mut App,
        instantiate_msg: multisig_prover::msg::InstantiateMsg,
    ) -> Addr {
        let code = ContractWrapper::new(
            multisig_prover::contract::execute,
            multisig_prover::contract::instantiate,
            multisig_prover::contract::query,
        )
        .with_reply(multisig_prover::contract::reply);
        let code_id = app.store_code(Box::new(code));

        app.instantiate_contract(
            code_id,
            Addr::unchecked("anyone"),
            &instantiate_msg,
            &[],
            "multisig_prover",
            None,
        )
        .unwrap()
    }
}
