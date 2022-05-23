// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority_active::gossip::configurable_batch_action_client::{
    init_configurable_authorities, BatchAction,
};
use std::time::Duration;
use tracing_test::traced_test;

#[tokio::test(flavor = "current_thread", start_paused = true)]
pub async fn test_gossip() {
    let action_sequence = vec![
        BatchAction::EmitUpdateItem(),
        //  BatchAction::EmitUpdateItem(),
        //  BatchAction::EmitUpdateItem(),
    ];

    let (clients, states, digests) = init_configurable_authorities(action_sequence).await;

    let mut active_authorities = Vec::new();
    // Start active processes.
    for state in states.clone() {
        let inner_state = state.clone();
        let inner_clients = clients.clone();

        let handle = tokio::task::spawn(async move {
            let active_state = ActiveAuthority::new(inner_state, inner_clients).unwrap();
            active_state.spawn_all_active_processes().await;
        });

        active_authorities.push(handle);
    }
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Expected outcome of gossip: each digest's tx signature and cert is now on every authority.
    let clients_final: Vec<_> = clients.values().collect();
    for client in clients_final.iter() {
        for digest in &digests {
            debug!("Checking the digest:   {:?}   ------ ", digest);
            let result1 = client
                .handle_transaction_info_request(TransactionInfoRequest {
                    transaction_digest: *digest,
                })
                .await;

            assert!(result1.is_ok());
            let result = result1.unwrap();
            let found_tx = result.signed_transaction.is_some();
            let found_cert = result.certified_transaction.is_some();
            debug!("found tx {:?}", found_tx);
            debug!("found cert {:?}", found_cert);
        }
    }
}

#[tokio::test]
#[traced_test]
pub async fn test_gossip_no_network() {
    info!("Start running test");

    // let (addr1, _) = get_key_pair();
    // let gas_object1 = Object::with_owner_for_testing(addr1);
    // let gas_object2 = Object::with_owner_for_testing(addr1);
    // let genesis_objects =
    //     authority_genesis_objects(4, vec![gas_object1.clone(), gas_object2.clone()]);
    //
    // let (aggregator, states) = init_configurable_authorities(a).await;
    //
    // // Connect to non-existing peer
    // let _aggregator = AuthorityAggregator::new(
    //     aggregator.committee.clone(),
    //     aggregator
    //         .authority_clients
    //         .iter()
    //         .map(|(name, _)| {
    //             let net = NetworkAuthorityClient::new(NetworkClient::new(
    //                 "127.0.0.1".to_string(),
    //                 // !!! This port does not exist
    //                 332,
    //                 65_000,
    //                 Duration::from_secs(1),
    //                 Duration::from_secs(1),
    //             ));
    //             (*name, net)
    //         })
    //         .collect(),
    // );
    //
    // let clients = aggregator.authority_clients.clone();
    //
    // // Start batch processes, and active processes.
    // if let Some(state) = states.into_iter().next() {
    //     let inner_state = state;
    //     let inner_clients = clients.clone();
    //
    //     let _active_handle = tokio::task::spawn(async move {
    //         let active_state = ActiveAuthority::new(inner_state, inner_clients).unwrap();
    //         active_state.spawn_all_active_processes().await
    //     });
    // }
    //
    // // Let the helper tasks start
    // tokio::task::yield_now().await;
    // tokio::time::sleep(Duration::from_secs(10)).await;
    //
    // // There have been timeouts and as a result the logs contain backoff messages
    // assert!(logs_contain("Waiting for 3.99"));
}
