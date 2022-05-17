use crate::authority::AuthorityState;
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::{AuthorityAPI, BatchInfoResponseItemStream};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::batch::{AuthorityBatch, SignedBatch, TxSequenceNumber, UpdateItem};
use sui_types::committee::{Committee, EpochId};
use sui_types::crypto::{get_key_pair, AuthoritySignature, KeyPair, PublicKeyBytes};
use sui_types::error::SuiError;
use sui_types::messages::{
    AccountInfoRequest, AccountInfoResponse, BatchInfoRequest, BatchInfoResponseItem,
    CertifiedTransaction, ConfirmationTransaction, ConsensusTransaction, ObjectInfoRequest,
    ObjectInfoResponse, Transaction, TransactionInfoRequest, TransactionInfoResponse,
};
use tokio::time::{Duration, Instant};

#[derive(Clone)]
pub struct TestBatch {
    pub start: TxSequenceNumber,
    pub digests: Vec<TransactionDigest>,
}

#[derive(Clone)]
pub enum BatchAction {
    DoNothing(Duration),
    EmitUpdateItems(TestBatch),
}

#[derive(Clone)]
pub struct ConfigurableBatchActionClient {
    state: Arc<AuthorityState>,
    pub action_sequence: Vec<BatchAction>,
}

impl ConfigurableBatchActionClient {
    #[cfg(test)]
    pub async fn new(committee: Committee, address: PublicKeyBytes, secret: KeyPair) -> Self {
        use crate::authority::AuthorityStore;
        use std::{env, fs};
        use sui_adapter::genesis;

        // Random directory
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        let store = Arc::new(AuthorityStore::open(path, None));
        let state = AuthorityState::new(
            committee.clone(),
            address,
            Arc::pin(secret),
            store,
            genesis::clone_genesis_compiled_modules(),
            &mut genesis::get_genesis_context(),
        )
        .await;

        ConfigurableBatchActionClient {
            state: Arc::new(state),
            action_sequence: Vec::new(),
        }
    }

    #[cfg(test)]
    pub async fn new_with_actions(
        committee: Committee,
        address: PublicKeyBytes,
        secret: KeyPair,
        actions: Vec<BatchAction>,
    ) -> Self {
        let mut client = Self::new(committee, address, secret).await;
        client.register_action_sequence(actions);
        client
    }

    #[cfg(test)]
    pub fn register_action_sequence(&mut self, action_sequence: Vec<BatchAction>) {
        self.action_sequence = action_sequence;
    }
}

#[async_trait]
impl AuthorityAPI for ConfigurableBatchActionClient {
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.state.clone();
        state.handle_transaction(transaction).await
    }

    async fn handle_confirmation_transaction(
        &self,
        _transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_consensus_transaction(
        &self,
        _transaction: ConsensusTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    async fn handle_account_info_request(
        &self,
        _request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        Ok(AccountInfoResponse {
            object_ids: vec![],
            owner: Default::default(),
        })
    }

    async fn handle_object_info_request(
        &self,
        _request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        Ok(ObjectInfoResponse {
            parent_certificate: None,
            requested_object_reference: None,
            object_and_lock: None,
        })
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        _request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        Ok(TransactionInfoResponse {
            signed_transaction: None,
            certified_transaction: None,
            signed_effects: None,
        })
    }

    /// Handle Batch information requests for this authority.
    async fn handle_batch_stream(
        &self,
        _request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, SuiError> {
        let mut last_batch = AuthorityBatch::initial();
        let actions = &self.action_sequence;
        let secret = self.state.secret.clone();
        let name = self.state.name;
        let mut items: Vec<Result<BatchInfoResponseItem, SuiError>> = Vec::new();

        let _ = actions.into_iter().for_each(|action| {
            match action {
                BatchAction::EmitUpdateItems(test_batch) => {
                    let mut temp_items: Vec<Result<BatchInfoResponseItem, SuiError>> = Vec::new();
                    let start_seq = test_batch.start;
                    let mut seq = start_seq;
                    let mut transactions = Vec::new();
                    for digest in test_batch.digests.clone() {
                        transactions.push((seq, digest));
                        // Safe client requires batches arrive first todo: discuss if that should be required.
                        temp_items.push(Ok(BatchInfoResponseItem(UpdateItem::Transaction((
                            seq, digest,
                        )))));
                        seq += 1;
                    }
                    let new_batch = AuthorityBatch::make_next(&last_batch, &transactions).unwrap();
                    last_batch = new_batch;
                    items.push({
                        let item = SignedBatch::new(last_batch.clone(), &*secret, name);
                        Ok(BatchInfoResponseItem(UpdateItem::Batch(item)))
                    });
                    for temp_item in temp_items {
                        items.push(temp_item)
                    }
                }
                BatchAction::DoNothing(_d) => {}
            };
        });

        Ok(Box::pin(tokio_stream::iter(items)))
    }
}

#[cfg(test)]
pub async fn init_configurable_authorities(
    authority_count: usize,
    authority_actions: Vec<Vec<BatchAction>>,
) -> (
    AuthorityAggregator<ConfigurableBatchActionClient>,
    Vec<Arc<AuthorityState>>,
) {
    let mut key_pairs = Vec::new();
    let mut voting_rights = BTreeMap::new();
    for _ in 0..authority_count {
        let (_, key_pair) = get_key_pair();
        let authority_name = *key_pair.public_key_bytes();
        voting_rights.insert(authority_name, 1);
        key_pairs.push((authority_name, key_pair));
    }
    let committee = Committee::new(0, voting_rights);

    let mut clients = BTreeMap::new();
    let mut states = Vec::new();
    for ((authority_name, secret), actions) in key_pairs.into_iter().zip(authority_actions) {
        let client = ConfigurableBatchActionClient::new_with_actions(
            committee.clone(),
            authority_name,
            secret,
            actions,
        )
        .await;
        states.push(client.state.clone());
        clients.insert(authority_name, client);
    }
    (AuthorityAggregator::new(committee, clients), states)
}
