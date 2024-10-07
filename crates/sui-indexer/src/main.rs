// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0
// use clap::Parser;
// use tokio_util::sync::CancellationToken;
// use tracing::warn;

// use sui_indexer::config::{Command, UploadOptions};
// use sui_indexer::database::ConnectionPool;
// use sui_indexer::db::{check_db_migration_consistency, reset_database, run_migrations};
// use sui_indexer::indexer::Indexer;
// use sui_indexer::metrics::{
//     spawn_connection_pool_metric_collector, start_prometheus_server, IndexerMetrics,
// };
// use sui_indexer::restorer::formal_snapshot::IndexerFormalSnapshotRestorer;
// use sui_indexer::sql_backfill::run_sql_backfill;
// use sui_indexer::store::PgIndexerStore;

// #[tokio::main]
// async fn main() -> anyhow::Result<()> {
//     let opts = sui_indexer::config::IndexerConfig::parse();

//     // NOTE: this is to print out tracing like info, warn & error.
//     let _guard = telemetry_subscribers::TelemetryConfig::new()
//         .with_env()
//         .init();
//     warn!("WARNING: Sui indexer is still experimental and we expect occasional breaking changes that require backfills.");

//     let (_registry_service, registry) = start_prometheus_server(opts.metrics_address)?;
//     mysten_metrics::init_metrics(&registry);
//     let indexer_metrics = IndexerMetrics::new(&registry);

//     let pool = ConnectionPool::new(
//         opts.database_url.clone(),
//         opts.connection_pool_config.clone(),
//     )
//     .await?;
//     spawn_connection_pool_metric_collector(indexer_metrics.clone(), pool.clone());

//     match opts.command {
//         Command::Indexer {
//             ingestion_config,
//             snapshot_config,
//             pruning_options,
//             upload_options,
//         } => {
//             // Make sure to run all migrations on startup, and also serve as a compatibility check.
//             run_migrations(pool.dedicated_connection().await?).await?;
//             let store = PgIndexerStore::new(pool, upload_options, indexer_metrics.clone());

//             Indexer::start_writer_with_config(
//                 &ingestion_config,
//                 store,
//                 indexer_metrics,
//                 snapshot_config,
//                 pruning_options,
//                 CancellationToken::new(),
//             )
//             .await?;
//         }
//         Command::JsonRpcService(json_rpc_config) => {
//             check_db_migration_consistency(&mut pool.get().await?).await?;

//             Indexer::start_reader(&json_rpc_config, &registry, pool).await?;
//         }
//         Command::ResetDatabase { force } => {
//             if !force {
//                 return Err(anyhow::anyhow!(
//                     "Resetting the DB requires use of the `--force` flag",
//                 ));
//             }

//             reset_database(pool.dedicated_connection().await?).await?;
//         }
//         Command::RunMigrations => {
//             run_migrations(pool.dedicated_connection().await?).await?;
//         }
//         Command::SqlBackFill {
//             sql,
//             checkpoint_column_name,
//             first_checkpoint,
//             last_checkpoint,
//             backfill_config,
//         } => {
//             run_sql_backfill(
//                 &sql,
//                 &checkpoint_column_name,
//                 first_checkpoint,
//                 last_checkpoint,
//                 pool,
//                 backfill_config,
//             )
//             .await;
//         }
//         Command::Restore(restore_config) => {
//             let store =
//                 PgIndexerStore::new(pool, UploadOptions::default(), indexer_metrics.clone());
//             let mut formal_restorer =
//                 IndexerFormalSnapshotRestorer::new(store, restore_config).await?;
//             formal_restorer.restore().await?;
//         }
//     }

//     Ok(())
// }

use fastcrypto::encoding::{Base58, Encoding};

fn match_base58_with_bytea(base58_string: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Step 1: Decode Base58 to raw bytes
    let bytes = Base58::decode(base58_string)?;

    // Step 2: Convert raw bytes to hex string
    let hex_string = hex::encode(bytes);

    // Step 3: Use in SQL query (example)
    let query = format!("SELECT * FROM your_table WHERE your_bytea_column = '\\x{}'", hex_string);

    Ok(query)
}

fn from_hex_to_base58(hex_string: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Step 1: Convert hex string to raw bytes
    let bytes = hex::decode(hex_string)?;

    // Step 2: Encode raw bytes to Base58
    let base58_string = Base58::encode(bytes);

    Ok(base58_string)
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let a = [3, 3, 4, 5, 6];
    let iter = a.into_iter().take_while(|n| *n == 3);
    println!("iter: {:?}", iter);
    let result: Vec<i32> = iter.take(5).collect();
    println!("result: {:?}", result);
    Ok(())
}