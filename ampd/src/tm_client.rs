use std::time::Duration;

use async_trait::async_trait;
use error_stack::{Report, Result};
use mockall::automock;
use tendermint::block::Height;
use tendermint_rpc::{Client, HttpClient};
use tokio::time;

pub type BlockResultsResponse = tendermint_rpc::endpoint::block_results::Response;
pub type BlockResponse = tendermint_rpc::endpoint::block::Response;
pub type Error = tendermint_rpc::Error;

#[automock]
#[async_trait]
pub trait TmClient {
    async fn latest_block(&self) -> Result<BlockResponse, Error>;
    async fn block_results(&self, block_height: Height) -> Result<BlockResultsResponse, Error>;
}

#[async_trait]
impl TmClient for HttpClient {
    async fn latest_block(&self) -> Result<BlockResponse, Error> {
        time::timeout(Duration::from_millis(2000), Client::latest_block(self))
            .await
            .expect("latest_block timed out")
            .map_err(Report::from)
    }

    async fn block_results(&self, height: Height) -> Result<BlockResultsResponse, Error> {
        time::timeout(Duration::from_millis(2000), Client::block_results(self, height))
            .await
            .expect("block_results timed out")
            .map_err(Report::from)
    }
}
