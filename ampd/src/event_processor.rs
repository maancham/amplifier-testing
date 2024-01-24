use core::future::Future;
use core::pin::Pin;
use std::{time::Duration, vec};

use crate::Error;
use async_trait::async_trait;
use error_stack::{bail, Context, Result, ResultExt};
use events::Event;
use futures::StreamExt;
use tokio::task::JoinSet;
use tokio::{select, time};
use tokio_stream::Stream;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::handlers::chain;

type Task = Box<dyn Future<Output = Result<(), EventProcessorError>> + Send>;

#[async_trait]
pub trait EventHandler {
    type Err: Context;

    async fn handle(&self, event: &Event) -> Result<(), Self::Err>;

    fn chain<H>(self, handler: H) -> chain::Handler<Self, H>
    where
        Self: Sized,
        H: EventHandler,
    {
        chain::Handler::new(self, handler)
    }
}

#[derive(Error, Debug)]
pub enum EventProcessorError {
    #[error("event handler failed handling event")]
    EventHandlerError,
    #[error("event stream error")]
    EventStreamError,
    #[error("handler failed unexpectedly")]
    HandlerFailed,
}

fn consume_events<H, S, E, L>(
    event_stream: S,
    label: L,
    handler: H,
    token: CancellationToken,
) -> Task
where
    H: EventHandler + Send + Sync + 'static,
    S: Stream<Item = Result<Event, E>> + Send + 'static,
    E: Context,
    L: AsRef<str> + Send + 'static,
{
    let task = async move {
        let mut event_stream = Box::pin(event_stream);
        while let Some(res) = event_stream.next().await {
            info!(
                "got event. label {:?} res is_ok() {:?} event stream size hint {:?}",
                label.as_ref(),
                res.is_ok(),
                event_stream.size_hint()
            );
            let event = res.change_context(EventProcessorError::EventStreamError)?;
            info!(
                "handling event. event {:?}, label {:?}",
                event,
                label.as_ref()
            );

            time::timeout(Duration::from_millis(20000), handler.handle(&event))
                .await
                .map_err(|err| {
                    info!("handler timed out");
                    err
                })
                .expect("handler timed out")
                .change_context(EventProcessorError::EventHandlerError)?;
            info!("handled event. label {:?}", label.as_ref());

            if matches!(event, Event::BlockEnd(_)) && token.is_cancelled() {
                info!("breaking in consume events. label {:?}", label.as_ref());
                break;
            }
            info!("waiting for next event. label {:?}", label.as_ref());
        }

        Ok(())
    };

    Box::new(task)
}

pub struct EventProcessor {
    tasks: Vec<Pin<Task>>,
    token: CancellationToken,
}

impl EventProcessor {
    pub fn new(token: CancellationToken) -> Self {
        EventProcessor {
            tasks: vec![],
            token,
        }
    }

    pub fn add_handler<H, S, E, L>(&mut self, label: L, handler: H, event_stream: S) -> &mut Self
    where
        H: EventHandler + Send + Sync + 'static,
        S: Stream<Item = Result<Event, E>> + Send + 'static,
        E: Context,
        L: AsRef<str> + Send + 'static,
    {
        self.tasks
            .push(consume_events(event_stream, label, handler, self.token.child_token()).into());
        self
    }

    pub async fn run(self) -> Result<(), EventProcessorError> {
        let mut set = JoinSet::new();
        let _abort_handles = self
            .tasks
            .into_iter()
            .map(|task| set.spawn(task))
            .collect::<Vec<_>>();

        Self::monitor_set(&mut set).await
    }

    async fn monitor_set(
        set: &mut JoinSet<Result<(), EventProcessorError>>,
    ) -> Result<(), EventProcessorError> {
        let mut interval = time::interval(5 * time::Duration::from_secs(5));
        loop {
            select! {
                result = set.join_next() =>  match result {
                Some(Ok(res)) => {
                        match res{
                        Ok(_) => info!("event processor task completed successfully"),
                            Err(_) => error!("event processor task completed with error")
                        }
                        return res
                }
                Some(Err(err)) =>{
                        error!("event processor task failed: {:?}", err);
                        bail!(EventProcessorError::HandlerFailed)
                    },
                None => panic!("all tasks exited unexpectedly"),
            },
                _ = interval.tick() =>
                    info!("currently {} event processor tasks running", set.len())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::event_processor::{EventHandler, EventProcessor};
    use async_trait::async_trait;
    use error_stack::{Report, Result};
    use futures::TryStreamExt;
    use mockall::mock;
    use thiserror::Error;
    use tokio::{self, sync::broadcast};
    use tokio_stream::wrappers::BroadcastStream;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn should_handle_events() {
        let event_count = 10;
        let (tx, rx) = broadcast::channel::<events::Event>(event_count);
        let token = CancellationToken::new();
        let mut processor = EventProcessor::new(token.child_token());

        let mut handler = MockEventHandler::new();
        handler
            .expect_handle()
            .returning(|_| Ok(()))
            .times(event_count);

        tokio::spawn(async move {
            for i in 0..event_count {
                assert!(tx.send(events::Event::BlockEnd((i as u32).into())).is_ok());
            }
        });

        processor.add_handler(
            "foo",
            handler,
            BroadcastStream::new(rx).map_err(Report::from),
        );
        assert!(processor.run().await.is_ok());
    }

    #[tokio::test]
    async fn should_return_error_if_handler_fails() {
        let (tx, rx) = broadcast::channel::<events::Event>(10);
        let token = CancellationToken::new();
        let mut processor = EventProcessor::new(token.child_token());

        let mut handler = MockEventHandler::new();
        handler
            .expect_handle()
            .returning(|_| Err(EventHandlerError::Unknown.into()))
            .once();

        tokio::spawn(async move {
            assert!(tx.send(events::Event::BlockEnd((10_u32).into())).is_ok());
        });

        processor.add_handler(
            "foo",
            handler,
            BroadcastStream::new(rx).map_err(Report::from),
        );
        assert!(processor.run().await.is_err());
    }

    #[tokio::test]
    async fn should_support_multiple_types_of_handlers() {
        let event_count = 10;
        let (tx, rx) = broadcast::channel::<events::Event>(event_count);
        let token = CancellationToken::new();
        let mut processor = EventProcessor::new(token.child_token());
        let stream = BroadcastStream::new(rx).map_err(Report::from);
        let another_stream = BroadcastStream::new(tx.subscribe()).map_err(Report::from);

        let mut handler = MockEventHandler::new();
        handler
            .expect_handle()
            .returning(|_| Ok(()))
            .times(event_count);

        let mut another_handler = MockAnotherEventHandler::new();
        another_handler
            .expect_handle()
            .returning(|_| Ok(()))
            .times(event_count);

        tokio::spawn(async move {
            for i in 0..event_count {
                assert!(tx.send(events::Event::BlockEnd((i as u32).into())).is_ok());
            }
        });

        processor.add_handler("foo", handler, stream).add_handler(
            "foo",
            another_handler,
            another_stream,
        );
        assert!(processor.run().await.is_ok());
    }

    #[derive(Error, Debug)]
    pub enum EventHandlerError {
        #[error("unknown")]
        Unknown,
    }

    mock! {
            EventHandler{}

            #[async_trait]
            impl EventHandler for EventHandler {
                type Err = EventHandlerError;

                async fn handle(&self, event: &events::Event) -> Result<(), EventHandlerError>;
            }
    }

    #[derive(Error, Debug)]
    pub enum AnotherEventHandlerError {}

    mock! {
            AnotherEventHandler{}

            #[async_trait]
            impl EventHandler for AnotherEventHandler {
                type Err = AnotherEventHandlerError;

                async fn handle(&self, event: &events::Event) -> Result<(), AnotherEventHandlerError>;
            }
    }
}
