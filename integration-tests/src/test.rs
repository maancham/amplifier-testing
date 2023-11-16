#[cfg(test)]
mod test {
    use axelar_wasm_std::{nonempty, voting};
    use connection_router::state::{ChainName, CrossChainId, Message};
    use cosmwasm_std::{
        coins, Addr, Binary, BlockInfo, Deps, Env, HexBinary, StdResult, Uint128, Uint256, Uint64,
    };
    use cw_multi_test::{App, ContractWrapper, Executor};
    use itertools::Itertools;

    use k256::ecdsa::{
        signature::Signer, signature::Verifier, signature::DigestSigner, RecoveryId, Signature, SigningKey, VerifyingKey,
    };
    use multisig::{key::PublicKey, signing};
    use rand_core::OsRng;
    use sha3::{Digest, digest::Update};

    const AXL_DENOMINATION: &str = "uaxl";
    #[test]
    fn test() {
        let genesis = Addr::unchecked("genesis");
        let mut app = App::new(|router, _, storage| {
            router
                .bank
                .init_balance(storage, &genesis, coins(10000000, AXL_DENOMINATION))
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
        let service_name: nonempty::String = "validators".to_string().try_into().unwrap();
        let workers = vec![Addr::unchecked("worker1"), Addr::unchecked("worker2")];
        let workers_with_keys = generate_keys(workers.clone());
        register_workers(
            &mut app,
            service_registry_address.clone(),
            multisig_address.clone(),
            service_name.clone(),
            governance_address.clone(),
            vec![
                "Ethereum".to_string().try_into().unwrap(),
                "Polygon".to_string().try_into().unwrap(),
            ],
            workers_with_keys.clone(),
            genesis.clone(),
        );
        let chain1 = instantiate_chain(
            &mut app,
            router_address.clone(),
            service_registry_address.clone(),
            rewards_address.clone(),
            multisig_address.clone(),
            governance_address.clone(),
            service_name.clone(),
            "Ethereum".to_string().try_into().unwrap(),
        );
        let chain2 = instantiate_chain(
            &mut app,
            router_address.clone(),
            service_registry_address.clone(),
            rewards_address.clone(),
            multisig_address.clone(),
            governance_address.clone(),
            service_name.clone(),
            "Polygon".to_string().try_into().unwrap(),
        );
        register_chain(
            &mut app,
            router_address.clone(),
            governance_address.clone(),
            chain1.clone(),
        );
        register_chain(
            &mut app,
            router_address,
            governance_address.clone(),
            chain2.clone(),
        );

        app.execute_contract(
            genesis,
            rewards_address.clone(),
            &rewards::msg::ExecuteMsg::AddRewards {
                contract_address: chain1.voting_verifier.to_string(),
            },
            &coins(1000, AXL_DENOMINATION),
        )
        .unwrap();
        let msg = Message {
            cc_id: CrossChainId {
                chain: chain1.chain_name.clone(),
                id: "foobar:2".to_string().try_into().unwrap(),
            },
            source_address: "some address".to_string().try_into().unwrap(),
            destination_address: "some other address".to_string().try_into().unwrap(),
            destination_chain: chain2.chain_name.clone(),
            payload_hash: [0; 32],
        };

        app.execute_contract(
            Addr::unchecked("relayer"),
            chain1.gateway.clone(),
            &gateway::msg::ExecuteMsg::VerifyMessages(vec![Message {
                cc_id: CrossChainId {
                    chain: chain1.chain_name.clone(),
                    id: "foobar:2".to_string().try_into().unwrap(),
                },
                source_address: "0xce16F69375520ab01377ce7B88f5BA8C48F8D666"
                    .to_string()
                    .try_into()
                    .unwrap(),
                destination_address: "0xce16F69375520ab01377ce7B88f5BA8C48F8D666"
                    .to_string()
                    .try_into()
                    .unwrap(),
                destination_chain: chain2.chain_name.clone(),
                payload_hash: [0; 32],
            }]),
            &[],
        )
        .unwrap();

        for worker in &workers {
            app.execute_contract(
                worker.clone(),
                chain1.voting_verifier.clone(),
                &voting_verifier::msg::ExecuteMsg::Vote {
                    poll_id: Uint64::one().into(),
                    votes: vec![true],
                },
                &[],
            )
            .unwrap();
        }
        app.execute_contract(
            Addr::unchecked("relayer"),
            chain1.voting_verifier.clone(),
            &voting_verifier::msg::ExecuteMsg::EndPoll {
                poll_id: Uint64::one().into(),
            },
            &[],
        )
        .unwrap();

        app.execute_contract(
            Addr::unchecked("relayer"),
            chain1.gateway,
            &gateway::msg::ExecuteMsg::RouteMessages(vec![Message {
                cc_id: CrossChainId {
                    chain: chain1.chain_name.clone(),
                    id: "foobar:2".to_string().try_into().unwrap(),
                },
                source_address: "0xce16F69375520ab01377ce7B88f5BA8C48F8D666"
                    .to_string()
                    .try_into()
                    .unwrap(),
                destination_address: "0xce16F69375520ab01377ce7B88f5BA8C48F8D666"
                    .to_string()
                    .try_into()
                    .unwrap(),
                destination_chain: chain2.chain_name,
                payload_hash: [0; 32],
            }]),
            &[],
        )
        .unwrap();

        let messages: Vec<Message> = app
            .wrap()
            .query_wasm_smart(
                chain2.gateway,
                &gateway::msg::QueryMsg::GetMessages {
                    message_ids: vec![CrossChainId {
                        chain: chain1.chain_name.clone(),
                        id: "foobar:2".to_string().try_into().unwrap(),
                    }],
                },
            )
            .unwrap();
        assert_eq!(messages.len(), 1);
        let old_block = app.block_info();
        app.set_block(BlockInfo {
            height: old_block.height + 20,
            ..old_block
        });

        app.execute_contract(
            Addr::unchecked("relayer"),
            rewards_address,
            &rewards::msg::ExecuteMsg::DistributeRewards {
                contract_address: chain1.voting_verifier.to_string(),
                epoch_count: None,
            },
            &[],
        );

        for worker in workers {
            let balance = app.wrap().query_balance(worker, AXL_DENOMINATION).unwrap();
            assert_eq!(balance.amount, Uint128::from(50u128));
        }

        let res = app
            .execute_contract(
                Addr::unchecked("relayer"),
                chain2.multisig_prover,
                &multisig_prover::msg::ExecuteMsg::ConstructProof {
                    message_ids: vec![msg.cc_id.to_string()],
                },
                &[],
            )
            .unwrap();
        println!("{:?}", res.events);
        let mut msg = "".to_string();
        for event in res.events {
            let attr = event.attributes.iter().find(|attr| attr.key == "msg");
            if attr.is_some() {
                msg = attr.clone().unwrap().value.clone();
            }
        }
        assert!(msg != "");
        for worker in workers_with_keys {
            println!("{}", HexBinary::from_hex(&msg).unwrap());
            println!("{:?}", worker.2);
            let msg_to_sign =
                sha3::Keccak256::new_with_prefix(HexBinary::from_hex(&msg).unwrap().as_slice());
            let finalized = msg_to_sign.clone().finalize();
            println!("{:?}",finalized);

            let signature : Signature =
                worker.1.sign_digest(msg_to_sign);
            //let signature : Signature= worker.1.sign(HexBinary::from_hex(&msg).unwrap().as_slice());
            let sig: String = hex::encode(signature.to_bytes().as_slice());
            let vk: VerifyingKey = VerifyingKey::from_sec1_bytes(worker.2.as_ref()).unwrap();

            println!("{:?}", sig);
            app.execute_contract(
                worker.0,
                multisig_address.clone(),
                &multisig::msg::ExecuteMsg::SubmitSignature {
                    session_id: Uint64::one(),
                    signature: HexBinary::from_hex(&sig).unwrap(),
                },
                &[],
            )
            .unwrap();
        }
    }
    fn register_chain(
        mut app: &mut App,
        router_address: Addr,
        governance_addr: Addr,
        chain: Chain,
    ) {
        app.execute_contract(
            governance_addr,
            router_address,
            &connection_router::msg::ExecuteMsg::RegisterChain {
                chain: chain.chain_name.clone(),
                gateway_address: chain.gateway.to_string(),
            },
            &[],
        )
        .unwrap();
    }

    fn generate_keys(worker_addresses: Vec<Addr>) -> Vec<(Addr, SigningKey, PublicKey)> {
        let mut workers = vec![];
        for worker in worker_addresses {
            let signing_key = SigningKey::random(&mut OsRng);
            let sk = signing_key.to_bytes();
            println!("\nSigning key: {:x?}", hex::encode(sk));
            let verify_key = VerifyingKey::from(&signing_key);
            // Serialize with `::to_encoded_point()`
            let vk = verify_key.to_encoded_point(false).to_bytes();
            let pk = HexBinary::from_hex(&hex::encode(vk)).unwrap();
            workers.push((worker, signing_key, PublicKey::Ecdsa(pk)));
        }
        workers
    }

    fn register_workers(
        mut app: &mut App,
        service_registry: Addr,
        multisig: Addr,
        service_name: nonempty::String,
        governance_addr: Addr,
        chains: Vec<ChainName>,
        workers: Vec<(Addr, SigningKey, PublicKey)>,
        genesis: Addr,
    ) {
        let min_worker_bond = Uint128::new(100);
        for worker in &workers {
            app.send_tokens(
                genesis.clone(),
                worker.0.clone(),
                &coins(min_worker_bond.u128(), AXL_DENOMINATION),
            )
            .unwrap();
        }
        let res = app.execute_contract(
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
        );
        assert!(res.is_ok());

        let res = app.execute_contract(
            governance_addr,
            service_registry.clone(),
            &service_registry::msg::ExecuteMsg::AuthorizeWorkers {
                workers: workers.iter().map(|w| w.0.to_string()).collect(),
                service_name: service_name.to_string(),
            },
            &[],
        );
        assert!(res.is_ok());
        for (addr, _, pk) in workers {
            let res = app.execute_contract(
                addr.clone(),
                service_registry.clone(),
                &service_registry::msg::ExecuteMsg::BondWorker {
                    service_name: service_name.to_string(),
                },
                &coins(min_worker_bond.u128(), AXL_DENOMINATION),
            );
            assert!(res.is_ok());

            let res = app.execute_contract(
                addr.clone(),
                service_registry.clone(),
                &service_registry::msg::ExecuteMsg::DeclareChainSupport {
                    service_name: service_name.to_string(),
                    chains: chains.clone(),
                },
                &[],
            );
            let res = app
                .execute_contract(
                    addr.clone(),
                    multisig.clone(),
                    &multisig::msg::ExecuteMsg::RegisterPublicKey { public_key: pk },
                    &[],
                )
                .unwrap();
        }
    }

    #[derive(Clone)]
    struct Chain {
        gateway: Addr,
        voting_verifier: Addr,
        multisig_prover: Addr,
        chain_name: ChainName,
        service_name: nonempty::String,
    }
    fn instantiate_chain(
        mut app: &mut App,
        router_address: Addr,
        service_registry_address: Addr,
        rewards_address: Addr,
        multisig_address: Addr,
        governance_address: Addr,
        service_name: nonempty::String,
        chain_name: ChainName,
    ) -> Chain {
        let voting_verifier = instantiate_voting_verifier(
            &mut app,
            voting_verifier::msg::InstantiateMsg {
                service_registry_address: service_registry_address.to_string().try_into().unwrap(),
                service_name: service_name.clone(),
                source_gateway_address: "doestry_into()n't matter".to_string().try_into().unwrap(),
                voting_threshold: (9, 10).try_into().unwrap(),
                block_expiry: 10,
                confirmation_height: 5,
                source_chain: chain_name.clone(),
                rewards_address: rewards_address.to_string(),
            },
        );
        let gateway = instantiate_gateway(
            &mut app,
            gateway::msg::InstantiateMsg {
                router_address: router_address.to_string(),
                verifier_address: voting_verifier.to_string(),
            },
        );
        let multisig_prover = instantiate_multisig_prover(
            &mut app,
            multisig_prover::msg::InstantiateMsg {
                admin_address: Addr::unchecked("doesn't matter").to_string(),
                gateway_address: gateway.to_string(),
                multisig_address: multisig_address.to_string(),
                service_registry_address: service_registry_address.to_string(),
                voting_verifier_address: voting_verifier.to_string(),
                destination_chain_id: Uint256::zero(),
                signing_threshold: (4, 5).try_into().unwrap(),
                service_name: service_name.to_string(),
                chain_name: chain_name.to_string(),
                worker_set_diff_threshold: 1,
                encoder: multisig_prover::encoding::Encoder::Abi,
                key_type: multisig::key::KeyType::Ecdsa,
            },
        );
        app.execute_contract(
            Addr::unchecked("doesn't matter"),
            multisig_prover.clone(),
            &multisig_prover::msg::ExecuteMsg::UpdateWorkerSet,
            &[],
        )
        .unwrap();
        app.execute_contract(
            governance_address,
            multisig_address,
            &multisig::msg::ExecuteMsg::AuthorizeCaller {
                contract_address: multisig_prover.clone(),
            },
            &[],
        )
        .unwrap();
        Chain {
            gateway,
            voting_verifier,
            multisig_prover,
            chain_name,
            service_name,
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
