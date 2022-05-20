// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::RpcFullNodeApiServer;
use crate::api::RpcReadApiServer;
use crate::rpc_gateway::responses::ObjectResponse;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use std::path::Path;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use sui_config::{NetworkConfig, PersistedConfig};
use sui_core::{
    authority::ReplicaStore,
    full_node::FullNodeState,
    gateway_types::{GetObjectInfoResponse, SuiObjectRef, TransactionEffectsResponse},
};
use sui_core::{
    authority_client::NetworkAuthorityClient, full_node::FullNode,
    gateway_state::GatewayTxSeqNumber,
};
use sui_storage::IndexStore;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    error::SuiError,
};
use tracing::info;

pub type FullNodeClient = Arc<FullNode<NetworkAuthorityClient>>;

pub struct SuiFullNode {
    pub client: FullNodeClient,
}

pub struct FullNodeReadApi {
    client: FullNodeClient,
}

pub async fn create_full_node_client(
    config_path: &Path,
    db_path: &Path,
) -> Result<FullNodeClient, anyhow::Error> {
    let network_config = PersistedConfig::read(config_path)?;
    // Start a full node
    let full_node = make_full_node(db_path, &network_config).await?;
    full_node.spawn_tasks().await;
    info!("Started full node ");
    Ok(Arc::new(full_node))
}

impl SuiFullNode {
    pub fn new(client: FullNodeClient) -> Self {
        Self { client }
    }
}

impl FullNodeReadApi {
    pub fn new(client: FullNodeClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl RpcReadApiServer for FullNodeReadApi {
    async fn get_owned_objects(&self, owner: SuiAddress) -> RpcResult<ObjectResponse> {
        let resp = ObjectResponse {
            objects: self
                .client
                .get_owned_objects(owner)
                .await?
                .iter()
                .map(|w| SuiObjectRef::from(*w))
                .collect(),
        };
        Ok(resp)
    }

    async fn get_object_info(&self, object_id: ObjectID) -> RpcResult<GetObjectInfoResponse> {
        Ok(self
            .client
            .get_object_info(object_id)
            .await?
            .try_into()
            .map_err(|e| anyhow!("{}", e))?)
    }
    async fn get_total_transaction_number(&self) -> RpcResult<u64> {
        Ok(self.client.state.get_total_transaction_number()?)
    }

    async fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.state.get_transactions_in_range(start, end)?)
    }

    async fn get_recent_transactions(
        &self,
        count: u64,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.state.get_recent_transactions(count)?)
    }

    async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> RpcResult<TransactionEffectsResponse> {
        Ok(self.client.state.get_transaction(digest).await?)
    }
}

#[async_trait]
impl RpcFullNodeApiServer for SuiFullNode {
    async fn get_transactions_by_input_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .client
            .state
            .get_transactions_by_input_object(object)
            .await?)
    }

    async fn get_transactions_by_mutated_object(
        &self,
        object: ObjectID,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self
            .client
            .state
            .get_transactions_by_mutated_object(object)
            .await?)
    }

    async fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.state.get_transactions_from_addr(addr).await?)
    }

    async fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
    ) -> RpcResult<Vec<(GatewayTxSeqNumber, TransactionDigest)>> {
        Ok(self.client.state.get_transactions_to_addr(addr).await?)
    }
}

pub async fn make_full_node(
    db_store_path: &Path,
    net_config: &NetworkConfig,
) -> Result<FullNode<NetworkAuthorityClient>, SuiError> {
    let store = Arc::new(ReplicaStore::open(db_store_path, None));
    let index_path = db_store_path.join("indexes");
    let indexes = Arc::new(IndexStore::open(index_path, None));

    let val_config = net_config
        .validator_configs()
        .iter()
        .next()
        .expect("Validtor set must be non empty");

    let follower_node_state = FullNodeState::new_with_genesis(
        net_config.committee(),
        store,
        indexes,
        val_config.genesis(),
    )
    .await?;

    let mut authority_clients = BTreeMap::new();
    let mut config = mysten_network::config::Config::new();
    config.connect_timeout = Some(Duration::from_secs(5));
    config.request_timeout = Some(Duration::from_secs(5));
    for validator in net_config
        .validator_configs()
        .iter()
        .next()
        .unwrap()
        .committee_config()
        .validator_set()
    {
        let channel = config.connect_lazy(validator.network_address()).unwrap();
        let client = NetworkAuthorityClient::new(channel);
        authority_clients.insert(validator.public_key(), client);
    }

    Ok(FullNode::new(Arc::new(follower_node_state), authority_clients).unwrap())
}
