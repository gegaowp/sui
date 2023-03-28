// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;
use sui_json_rpc::api::{WriteApiClient, WriteApiServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    BigInt, DevInspectResults, DryRunTransactionResponse, SuiTransactionResponse,
    SuiTransactionResponseOptions,
};
use sui_open_rpc::Module;
use sui_types::base_types::{EpochId, SuiAddress};
use sui_types::messages::ExecuteTransactionRequestType;

use crate::models::transactions::Transaction;
use crate::store::IndexerStore;
use crate::types::{
    FastPathTransactionResponse, SuiTransactionResponseWithOptions,
    TemporaryTransactionResponseStore,
};

pub(crate) struct WriteApi<S> {
    fullnode: HttpClient,
    state: S,
}

impl<S: IndexerStore> WriteApi<S> {
    pub fn new(state: S, fullnode_client: HttpClient) -> Self {
        Self {
            state,
            fullnode: fullnode_client,
        }
    }
}

#[async_trait]
impl<S> WriteApiServer for WriteApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    async fn execute_transaction(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        options: Option<SuiTransactionResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> RpcResult<SuiTransactionResponse> {
        let fast_path_options = SuiTransactionResponseOptions::full_content();
        let sui_transaction_response = self
            .fullnode
            .execute_transaction(tx_bytes, signatures, Some(fast_path_options), request_type)
            .await?;

        let fast_path_resp: FastPathTransactionResponse =
            sui_transaction_response.clone().try_into()?;
        let transaction_store: TemporaryTransactionResponseStore = fast_path_resp.into();
        let transaction: Transaction = transaction_store.try_into()?;
        self.state.persist_fast_path(transaction)?;

        Ok(SuiTransactionResponseWithOptions {
            response: sui_transaction_response,
            options: options.unwrap_or_default(),
        }
        .into())
    }

    async fn dev_inspect_transaction(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt>,
        epoch: Option<EpochId>,
    ) -> RpcResult<DevInspectResults> {
        self.fullnode
            .dev_inspect_transaction(sender_address, tx_bytes, gas_price, epoch)
            .await
    }

    async fn dry_run_transaction(&self, tx_bytes: Base64) -> RpcResult<DryRunTransactionResponse> {
        self.fullnode.dry_run_transaction(tx_bytes).await
    }
}

impl<S> SuiRpcModule for WriteApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::WriteApiOpenRpc::module_doc()
    }
}
