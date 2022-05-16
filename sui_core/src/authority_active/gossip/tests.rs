// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority_active::gossip::configurable_batch_action_client::{
    init_configurable_authorities, BatchAction, TestBatch,
};
use crate::authority_client::NetworkAuthorityClient;
use futures::future::join_all;
use std::time::Duration;
use sui_network::network::NetworkClient;
use sui_types::base_types::TransactionDigest;
use sui_types::object::Object;
use tokio::runtime;
use tracing_test::traced_test;

#[tokio::test()]
pub async fn test_gossip() {
    let authority_count = 4;
    let digest1 = TransactionDigest::random();
    let digest2 = TransactionDigest::random();
    let digest3 = TransactionDigest::random();
    let digests = vec![digest1, digest2, digest3];
    let action_sequence = vec![BatchAction::EmitUpdateItems(TestBatch {
        start: 1,
        digests,
    })];
    let action_sequences = vec![
        action_sequence.clone(),
        action_sequence.clone(),
        action_sequence.clone(),
        vec![],
    ];
    let (aggregator, states) =
        init_configurable_authorities(authority_count, action_sequences).await;

    let clients = aggregator.authority_clients.clone();

    let mut active_authorities = Vec::new();
    // Start active processes.
    for state in states.clone() {
        let inner_state = state.clone();
        let inner_clients = clients.clone();

        //let active_handle = tokio::task::spawn(async move {
        let active_state = ActiveAuthority::new(inner_state, inner_clients).unwrap();
        let tx_cancellation = active_state.spawn_all_active_processes().await;
        //});
        active_authorities.push(tx_cancellation);
    }

    //tokio::task::yield_now().await;
    //tokio::time::sleep(Duration::from_secs(1)).await;

    for state in states {
        let result1 = state._database.transaction_exists(&digest1);
        let result2 = state._database.transaction_exists(&digest2);
        let result3 = state._database.transaction_exists(&digest3);

        assert!(result1.is_ok());
        assert!(result1.unwrap());
        assert!(result2.is_ok());
        assert!(result2.unwrap());
        assert!(result3.is_ok());
        assert!(result3.unwrap());
    }
    for active in active_authorities {
        _ = active.send(());
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
