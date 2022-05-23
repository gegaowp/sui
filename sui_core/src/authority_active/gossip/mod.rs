// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    authority::AuthorityState, authority_aggregator::AuthorityAggregator,
    authority_client::AuthorityAPI, safe_client::SafeClient,
};
use futures::{stream::FuturesUnordered, StreamExt};
use itertools::Itertools;
use std::collections::HashMap;
use std::{collections::HashSet, sync::Arc, time::Duration};
use sui_types::committee::Committee;
use sui_types::messages::TransactionInfoResponse;
use sui_types::{
    base_types::AuthorityName,
    batch::{TxSequenceNumber, UpdateItem},
    error::SuiError,
    messages::{
        BatchInfoRequest, BatchInfoResponseItem, ConfirmationTransaction, TransactionInfoRequest,
    },
};
use tokio::sync::oneshot::Receiver;
use tracing::{debug, error, info};

#[cfg(test)]
mod configurable_batch_action_client;
// #[cfg(test)]
// mod test_batch_action;
mod peer_gossip;
#[cfg(test)]
mod tests;

struct PeerGossip<A> {
    peer_name: AuthorityName,
    client: SafeClient<A>,
    state: Arc<AuthorityState>,
    max_seq: Option<TxSequenceNumber>,
    aggregator: Arc<AuthorityAggregator<A>>,
}

const EACH_ITEM_DELAY_MS: u64 = 1_000;
const REQUEST_FOLLOW_NUM_DIGESTS: u64 = 100_000;
const REFRESH_FOLLOWER_PERIOD_SECS: u64 = 60;

use super::ActiveAuthority;

pub async fn gossip_process<A>(active_authority: &ActiveAuthority<A>, degree: usize)
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // A copy of the committee
    let committee = &active_authority.net.committee;

    // Number of tasks at most "degree" and no more than committee - 1
    let target_num_tasks: usize = usize::min(
        active_authority.state.committee.voting_rights.len() - 1,
        degree,
    );

    // If we do not expect to connect to anyone
    if target_num_tasks == 0 {
        info!("Turning off gossip mechanism");
        return;
    }
    info!("Turning on gossip mechanism");

    // Keep track of names of active peers
    let mut peer_names = HashSet::new();
    let mut gossip_tasks = FuturesUnordered::new();

    loop {
        let mut k = 0;
        while gossip_tasks.len() < target_num_tasks {
            // Find out what is the earliest time that we are allowed to reconnect
            // to at least 2f+1 nodes.
            let next_connect = active_authority
                .minimum_wait_for_majority_honest_available()
                .await;
            debug!(
                "Waiting for {:?}",
                next_connect - tokio::time::Instant::now()
            );
            tokio::time::sleep_until(next_connect).await;

            let name_result = select_gossip_peer(
                active_authority.state.name,
                peer_names.clone(),
                active_authority,
            )
            .await;
            if name_result.is_err() {
                continue;
            }
            let name = name_result.unwrap();

            peer_names.insert(name);
            gossip_tasks.push(async move {
                let peer_gossip = PeerGossip::new(name, active_authority);
                // Add more duration if we make more than 1 to ensure overlap
                debug!("Starting gossip from peer {:?}", name);
                peer_gossip
                    .spawn(Duration::from_secs(REFRESH_FOLLOWER_PERIOD_SECS + k * 15))
                    .await
            });
            k += 1;

            // If we have already used all the good stake, then stop here and
            // wait for some node to become available.
            let total_stake_used: usize = peer_names
                .iter()
                .map(|name| committee.weight(name))
                .sum::<usize>()
                + committee.weight(&active_authority.state.name);
            if total_stake_used >= committee.quorum_threshold() {
                break;
            }
        }

        // If we have no peers no need to wait for one
        if gossip_tasks.is_empty() {
            continue;
        }

        // Let the peer gossip task finish
        let (finished_name, _result) = gossip_tasks.select_next_some().await;
        if let Err(err) = _result {
            active_authority.set_failure_backoff(finished_name).await;
            error!("Peer {:?} returned error: {:?}", finished_name, err);
        } else {
            active_authority.set_success_backoff(finished_name).await;
            debug!("End gossip from peer {:?}", finished_name);
        }
        peer_names.remove(&finished_name);
    }
}

pub async fn select_gossip_peer<A>(
    my_name: AuthorityName,
    peer_names: HashSet<AuthorityName>,
    active_authority: &ActiveAuthority<A>,
) -> Result<AuthorityName, SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Make sure we exit loop by limiting the number of tries to choose peer
    // where n is the total number of committee members.
    let mut tries_remaining = active_authority.state.committee.voting_rights.len();
    loop {
        let name = active_authority.state.committee.sample();
        if peer_names.contains(name)
            || *name == my_name
            || !active_authority.can_contact(*name).await
        {
            tries_remaining -= 1;
            if tries_remaining == 0 {
                return Err(SuiError::GenericAuthorityError {
                    error: "Could not connect to any peer".to_string(),
                });
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        }
        return Ok(*name);
    }
}

impl<A> PeerGossip<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(peer_name: AuthorityName, active_authority: &ActiveAuthority<A>) -> PeerGossip<A> {
        PeerGossip {
            peer_name,
            client: active_authority.net.authority_clients[&peer_name].clone(),
            state: active_authority.state.clone(),
            max_seq: None,
            aggregator: active_authority.net.clone(),
        }
    }

    pub async fn spawn(mut self, duration: Duration) -> (AuthorityName, Result<(), SuiError>) {
        let peer_name = self.peer_name;
        let result =
            tokio::task::spawn(async move { self.peer_gossip_for_duration(duration).await }).await;

        if result.is_err() {
            return (
                peer_name,
                Err(SuiError::GenericAuthorityError {
                    error: "Gossip Join Error".to_string(),
                }),
            );
        };

        (peer_name, result.unwrap())
    }

    async fn peer_gossip_for_duration(&mut self, duration: Duration) -> Result<(), SuiError> {
        // Global timeout, we do not exceed this time in this task.
        let mut timeout = Box::pin(tokio::time::sleep(duration));

        let req = BatchInfoRequest {
            start: self.max_seq,
            length: REQUEST_FOLLOW_NUM_DIGESTS,
        };

        let mut streamx = Box::pin(self.client.handle_batch_stream(req).await?);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    // No matter what happens we do not spend too much time on any peer.
                    break;
                },

                items = &mut streamx.next() => {
                    match items {
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Batch(_signed_batch)) )) => {},

                        // Upon receiving a transaction digest, store it if it is not processed already.
                        Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, digest))))) => {
                            if !self.state._database.transaction_exists(&digest)? {
                                // Download the certificate
                                debug!("Digest {:?} is getting downloaded", digest);
                                let response = self.client.handle_transaction_info_request(TransactionInfoRequest::from(digest)).await?;
                                self.process_response(response).await?;

                            }
                            self.max_seq = Some(seq + 1);

                        },

                        // Return any errors.
                        Some(Err( err )) => {
                            debug!("error while reading from stream {:?}", err);
                            return Err(err);
                        },

                        // The stream has closed, re-request:
                        None => {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            let req = BatchInfoRequest {
                                start: self.max_seq,
                                length: REQUEST_FOLLOW_NUM_DIGESTS,
                            };
                            streamx = Box::pin(self.client.handle_batch_stream(req).await?);
                        },
                    }
                },
            };
        }
        Ok(())
    }

    async fn process_response(&self, response: TransactionInfoResponse) -> Result<(), SuiError> {
        if let Some(certificate) = response.certified_transaction {
            // Process the certificate from one authority to ourselves
            self.aggregator
                .sync_authority_source_to_destination(
                    ConfirmationTransaction { certificate },
                    self.peer_name,
                    self.state.name,
                )
                .await?;
            Ok(())
        } else {
            // The authority did not return the certificate, despite returning info
            // But it should know the certificate!
            Err(SuiError::ByzantineAuthoritySuspicion {
                authority: self.peer_name,
            })
        }
    }
}
