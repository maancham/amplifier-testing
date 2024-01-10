use std::{fmt::Debug, time::Duration};

use error_stack::Report;
use ethers::providers::{
    Http, HttpRateLimitRetryPolicy, JsonRpcClient, ProviderError, RetryClient, RetryClientBuilder,
    RpcError,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::url::Url;

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
        self.provider
            .request(method, params)
            .await
            .map_err(Into::into)
            .map_err(Report::from)
    }
}

impl Client<Http> {
    pub fn new_http(url: &Url) -> Result<Self> {
        Ok(Client::new(Http::new_with_client(
            url,
            reqwest::Client::builder()
                .timeout(Duration::from_millis(2000))
                .connect_timeout(Duration::from_millis(2000))
                .build()
                .unwrap(),
        )))
    }
}
