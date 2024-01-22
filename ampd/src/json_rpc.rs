use std::{fmt::Debug, time::Duration};

use error_stack::Report;
use ethers::providers::{Http, JsonRpcClient, ProviderError};
use serde::{de::DeserializeOwned, Serialize};

use crate::url::Url;
use tokio::time;

use axelar_wasm_std::utils::InspectorResult;
use tracing::info;

type Result<T> = error_stack::Result<T, ProviderError>;

pub struct Client<P>
where
    P: JsonRpcClient,
{
    provider: P,
}

impl<P> Client<P>
where
    P: JsonRpcClient,
{
    pub fn new(provider: P) -> Self {
        Client { provider }
    }

    pub async fn request<T, R>(&self, method: &str, params: T) -> Result<R>
    where
        T: Debug + Serialize + Send + Sync,
        R: DeserializeOwned + Send,
    {
        info!("sending rpc client request");
        time::timeout(
            Duration::from_millis(2000),
            self.provider.request(method, params),
        )
        .await
        .tap(|_| info!("got rpc client response"))
        .map_err(|err| {
            info!("eth json RPC timed out");
            err
        })
        .expect("eth json RPC timed out")
        .map_err(Into::into)
        .map_err(Report::from)
    }
}

impl Client<Http> {
    pub fn new_http(url: &Url) -> Result<Self> {
        Ok(Client::new(Http::new(url)))
    }
}
