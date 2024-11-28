use futures::StreamExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use surrealdb::engine::local::{Db, Mem};
use surrealdb::method::Stream;
use surrealdb::{Action, Surreal};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, trace};
use uuid::Uuid;

pub type Subscription<T> = mpsc::Receiver<T>;

#[derive(Serialize, Deserialize, Debug)]
struct Topic {
    name: String,
    capacity: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Item<T> {
    item_id: String,
    item: T,
}

#[derive(Serialize, Deserialize, Debug)]
struct Sub {
    topic: String,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("UnableToInitializeBus")]
    UnableToInitializeEventBus,
    #[error("InternalLookupError")]
    InternalLookupError,
    #[error("TopicExists: {0}")]
    TopicExists(String),
    #[error("TopicDoesNotExist: {0}")]
    TopicDoesNotExist(String),
    #[error("TopicDeleteError")]
    TopicDeleteError,
    #[error("TopicCreateError")]
    TopicCreateError,
    #[error("SubscriptionIdNotFound")]
    SubscriptionDoesNotExist,
    #[error("UnsubscribeError")]
    UnsubscribeError,
}

#[derive(Debug, Error)]
pub enum TryPublishError<T> {
    #[error("Full")]
    Full(T),
    #[error("TopicDoesNotExist")]
    TopicDoesNotExist,
    #[error("InternalLookupError")]
    InternalLookupError,
}

#[derive(Clone)]
pub struct Connected {}

#[derive(Clone)]
pub struct Disconnnected {}

#[derive(Clone)]
pub struct EventBus<State = Disconnnected> {
    db: Surreal<Db>,
    state: std::marker::PhantomData<State>,
}

impl EventBus<Disconnnected> {
    pub async fn new() -> Result<EventBus<Connected>, Error> {
        let db = Surreal::new::<Mem>(()).await.map_err(|e| {
            error!(
                err = "UnableToInitializeEventBus",
                "unable to initialize event bus"
            );
            trace!(err = format!("{}", e), "unable to initialize event bus");
            Error::UnableToInitializeEventBus
        })?;

        db.use_ns("eventbus")
            .use_db("eventbus")
            .await
            .map_err(|e| {
                error!(
                    err = "UnableToInitializeEventBus",
                    "unable to initialize event bus"
                );
                trace!(err = format!("{}", e), "unable to initialize event bus");
                Error::UnableToInitializeEventBus
            })?;

        Ok(EventBus {
            db,
            state: std::marker::PhantomData,
        })
    }
}

impl EventBus<Connected> {
    pub async fn create_topic(&self, topic: &str, capacity: Option<usize>) -> Result<(), Error> {
        let _: Option<Topic> = self
            .db
            .create(("topics", topic))
            .content(Topic {
                name: topic.into(),
                capacity,
            })
            .await
            .map_err(|e| {
                if let surrealdb::Error::Db(e) = e {
                    match e {
                        surrealdb::error::Db::RecordExists { thing: _ } => {
                            error!(err = "TopicExists", topic = topic, "topic exists");
                            Error::TopicExists(topic.into())
                        }
                        _ => {
                            error!(
                                err = "TopicCreateError",
                                topic = topic,
                                "failed to create topic"
                            );
                            trace!(
                                err = format!("{}", e),
                                topic = topic,
                                "failed to create topic"
                            );
                            Error::TopicCreateError
                        }
                    }
                } else {
                    error!(
                        err = "TopicCreateError",
                        topic = topic,
                        "failed to create topic"
                    );
                    trace!(
                        err = format!("{}", e),
                        topic = topic,
                        "failed to create topic"
                    );
                    Error::TopicCreateError
                }
            })?;

        info!(topic = topic, "topic created");
        Ok(())
    }

    pub async fn delete_topic(&self, topic: &str) -> Result<(), Error> {
        let t: Option<Topic> = self.db.delete(("topics", topic)).await.map_err(|e| {
            error!(
                err = "InternalLookupError",
                topic = topic,
                "failed to delete topic"
            );
            trace!(
                err = format!("{}", e),
                topic = topic,
                "failed to delete topic"
            );
            Error::InternalLookupError
        })?;

        if t.is_none() {
            error!(
                err = "TopicDoesNotExist",
                topic = topic,
                "failed to delete topic"
            );
            return Err(Error::TopicDoesNotExist(topic.into()));
        }

        let _v = self
            .db
            .query(format!("DELETE type::table('{}');", topic))
            .await
            .map_err(|e| {
                error!(
                    err = "TopicDeleteError",
                    topic = topic,
                    "failed to delete topic"
                );
                trace!(
                    err = format!("{}", e),
                    topic = topic,
                    "failed to delete topic"
                );
                Error::TopicDeleteError
            })?;

        info!(topic = topic, "topic deleted");
        Ok(())
    }

    pub async fn unsubscribe(&self, id: &str) -> Result<(), Error> {
        let _t: Option<Sub> = self.db.delete(("subscriptions", id)).await.map_err(|e| {
            if let surrealdb::Error::Db(e) = e {
                match e {
                    surrealdb::error::Db::NoRecordFound => {
                        error!(
                            err = "SubscriptionDoesNotExist",
                            subscription_id = id,
                            "failed to unsubscribe"
                        );
                        Error::SubscriptionDoesNotExist
                    }
                    _ => {
                        error!(subscription_id = id, "failed to unsubscribe");
                        trace!(
                            err = format!("{}", e),
                            subscription_id = id,
                            "failed to unsubscribe"
                        );
                        Error::UnsubscribeError
                    }
                }
            } else {
                error!(
                    err = "UnsubscribeError",
                    subscription_id = id,
                    "failed to unsubscribe"
                );
                trace!(
                    err = format!("{}", e),
                    subscription_id = id,
                    "failed to unsubscribe"
                );
                Error::UnsubscribeError
            }
        })?;

        debug!(subscription_id = id, "unsubscribe success");
        Ok(())
    }

    pub async fn count<T: Serialize + DeserializeOwned + 'static>(
        &self,
        topic: &str,
    ) -> Result<usize, Error> {
        let items: Vec<Item<T>> = self.db.select(topic).await.map_err(|e| {
            if let surrealdb::Error::Db(e) = e {
                match e {
                    surrealdb::error::Db::NoRecordFound => {
                        error!(
                            err = "TopicDoesNotExist",
                            topic = topic,
                            "failed to provide count"
                        );
                        Error::TopicDoesNotExist(topic.into())
                    }
                    _ => {
                        error!(topic = topic, "failed to provide count");
                        trace!(
                            err = format!("{}", e),
                            topic = topic,
                            "failed to provide count"
                        );
                        Error::InternalLookupError
                    }
                }
            } else {
                error!(
                    err = "InternalLookupError",
                    topic = topic,
                    "failed to provide count"
                );
                trace!(
                    err = format!("{}", e),
                    topic = topic,
                    "failed to provide count"
                );
                Error::InternalLookupError
            }
        })?;

        trace!(topic = topic, count = items.len(), "topic event count");
        Ok(items.len())
    }

    pub async fn publish<T: Serialize + DeserializeOwned + 'static>(
        &self,
        topic: &str,
        item: T,
    ) -> Result<(), Error> {
        debug!(topic = topic, "publishing message");

        let mut i = item;

        loop {
            match self.try_publish(topic, i).await {
                Ok(_) => return Ok(()),
                Err(e) => match e {
                    TryPublishError::Full(event) => {
                        i = event;
                        sleep(Duration::from_millis(50)).await;
                    }
                    TryPublishError::InternalLookupError => return Err(Error::InternalLookupError),
                    TryPublishError::TopicDoesNotExist => {
                        return Err(Error::TopicDoesNotExist(topic.into()))
                    }
                },
            }
        }
    }

    pub async fn try_publish<T: Serialize + DeserializeOwned + 'static>(
        &self,
        topic: &str,
        item: T,
    ) -> Result<(), TryPublishError<T>> {
        trace!(topic = topic, "publishing message");
        let ct: Option<Topic> = self
            .db
            .select(("topics", topic))
            .await
            .map_err(|_e| TryPublishError::InternalLookupError)?;

        let t = match ct {
            Some(t) => t,
            None => return Err(TryPublishError::TopicDoesNotExist),
        };

        if let Some(cap) = t.capacity {
            let r: Vec<Item<T>> = match self.db.select(topic).await {
                Err(_e) => return Err(TryPublishError::InternalLookupError),
                Ok(i) => i,
            };
            if r.len() >= cap {
                return Err(TryPublishError::Full(item));
            }
        }

        let id: String = Uuid::new_v4().into();

        let i = Item {
            item_id: id.clone(),
            item,
        };

        let _: Option<Item<T>> = self
            .db
            .create((topic, id))
            .content(i)
            .await
            .map_err(|_e| TryPublishError::InternalLookupError)?;

        Ok(())
    }

    pub async fn subscribe<
        T: Send + Serialize + DeserializeOwned + std::marker::Unpin + 'static,
    >(
        &self,
        topic: &str,
    ) -> Result<(String, Subscription<T>), Error> {
        let exists = self
            .db
            .select::<Option<Topic>>(("topics", topic))
            .await
            .map_err(|_e| Error::InternalLookupError)?
            .is_some();

        if !exists {
            return Err(Error::TopicDoesNotExist(topic.into()));
        };

        let subscriber_id: String = Uuid::new_v4().into();
        debug!(
            topic = topic,
            subscription_id = subscriber_id,
            "subscription created"
        );

        let _: Option<Sub> = self
            .db
            .create(("subscriptions", &subscriber_id))
            .content(Sub {
                topic: topic.to_string(),
            })
            .await
            .map_err(|_e| Error::InternalLookupError)?;

        let _relate = self
            .db
            .query(format!(
                "RELATE subscriptions:`{0}`->subscribe->topics:`{1}`;",
                subscriber_id, topic
            ))
            .await
            .unwrap();

        let current_events: Vec<Item<T>> = self.db.select(topic).await.unwrap();

        trace!(
            "Received {} existing events from {}",
            current_events.len(),
            topic
        );

        let mut stream = self
            .db
            .select(topic)
            .live()
            .await
            .map_err(|_e| Error::InternalLookupError)?;

        let mut monitor: Stream<Option<Sub>> = self
            .db
            .select(("subscriptions", &subscriber_id))
            .live()
            .await
            .map_err(|_e| Error::InternalLookupError)?;

        let sub_id = subscriber_id.clone();

        let (sender, receiver) = mpsc::channel(1);
        let db = self.db.clone();
        let t = topic.to_string();

        tokio::spawn(async move {
            trace!("in subscription handle");
            let terminating_closure = async {
                trace!(
                    subscription_id = subscriber_id,
                    topic = t,
                    "starting terminating handle"
                );
                db
                    // remove from subscribers
                    .query(format!("DELETE subscriptions:`{}`;", subscriber_id))
                    // update any pending events for subscriber
                    .query(format!("FOR $item IN (SELECT id FROM {0}) {{
    let $subs = (SELECT <-subscribe.in AS t FROM type::thing('topics', '{0}'));
    let $recv = (SELECT <-received.in AS t FROM type::thing('{0}', $item.id));
    IF array::is_empty(array::difference($subs.t, $recv.t)) THEN DELETE type::thing('{0}', $item.id) END;
}};", t))
                    .await.unwrap();
            };

            for i in current_events {
                trace!(
                    subscription_id = subscriber_id,
                    topic = t,
                    "sending existing"
                );
                let mut data = i.item;
                while let Err(e) = sender.try_send(data) {
                    match e {
                        TrySendError::Full(d) => {
                            data = d;
                            sleep(Duration::from_millis(50)).await;
                        }
                        TrySendError::Closed(_) => {
                            terminating_closure.await;
                            return;
                        }
                    }
                }

                acknowledge_message_id(&db, &t, &subscriber_id, &i.item_id)
                    .await
                    .unwrap();
            }

            loop {
                tokio::select! {
                    _ = sender.closed() => {
                        trace!(subscription_id = subscriber_id, topic = t, "received channel closed event");
                        terminating_closure.await;
                        return
                    },
                    _ = monitor.next() => {
                        trace!(subscription_id = subscriber_id, topic = t, "received unsubscribe event");
                        terminating_closure.await;
                        return
                    },
                    s = stream.next() => {
                        trace!(subscription_id = subscriber_id, topic = t, "received message from stream");
                        if let Some(Ok(item)) = s {
                            if item.action == Action::Create {
                                let i: Item<T> = item.data;
                                let mut data = i.item;

                                while let Err(e) = sender.try_send(data) {
                                    trace!(subscription_id = subscriber_id, topic = t, message_id = i.item_id, "sending message to channel");
                                    match e {
                                        TrySendError::Full(d) => {
                                            trace!(subscription_id = subscriber_id, topic = t, message_id = i.item_id, "channel is full");
                                            data = d;
                                            sleep(Duration::from_millis(50)).await;
                                        },
                                        TrySendError::Closed(_) => {
                                            trace!(subscription_id = subscriber_id, topic = t, message_id = i.item_id, "sending on closed channel");
                                            terminating_closure.await;
                                            return
                                        }
                                    }
                                }

                                acknowledge_message_id(&db, &t, &subscriber_id, &i.item_id).await.unwrap();
                            };

                        } else {
                            error!(subscription_id = subscriber_id, topic = t, "i don't know what happens here");
                            terminating_closure.await;
                            return
                        }
                    }
                }
            }
        });

        Ok((sub_id, receiver))
    }
}

async fn acknowledge_message_id(
    db: &Surreal<Db>,
    topic: &str,
    subscriber_id: &str,
    message_id: &str,
) -> Result<(), Error> {
    trace!(
        subscription_id = subscriber_id,
        topic = topic,
        message_id = message_id,
        "acknowledging message"
    );
    match db
        // acknowledge message received by subscriber
        .query(format!(
            "LET $table = type::thing('{1}', '{2}'); RELATE subscriptions:`{0}`->received->$table;",
            subscriber_id, topic, message_id
        ))
        // delete message if received by all subscribers when published
        .query(format!(
            "LET $table = type::thing('{0}', '{1}');
LET $recv = (SELECT <-received.in AS t FROM $table);
LET $subs = (SELECT <-subscribe.in AS t FROM topics:`{0}`);
IF array::is_empty(array::difference($subs.t, $recv.t)) THEN DELETE $table END;",
            topic, message_id
        ))
        .await
    {
        Ok(_i) => Ok(()),
        Err(_e) => Err(Error::InternalLookupError),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct Testing {
        name: String,
    }

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct Relationship {}

    #[tokio::test]
    async fn create_duplicate_topic() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name, None).await.unwrap();
        match bus.create_topic(&name, None).await {
            Ok(_) => panic!("Should have failed"),
            Err(e) => match e {
                Error::TopicExists(s) => assert_eq!(s, name),
                _ => panic!("Expected TopicExists error received {:?}", e),
            },
        }
    }

    #[tokio::test]
    async fn topic_lifecycle() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();

        // ensure topic exists
        bus.create_topic(&name, None).await.unwrap();
        let ct: Vec<Topic> = bus.db.select("topics").await.unwrap();
        assert_eq!(ct.len(), 1);

        // ensure topic does not exist
        bus.delete_topic(&name).await.unwrap();
        let ct: Vec<Topic> = bus.db.select("topics").await.unwrap();
        assert_eq!(ct.len(), 0)
    }

    #[tokio::test]
    async fn delete_topic_does_not_exist() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        match bus.delete_topic(&name).await {
            Ok(_) => panic!("Expected error but received none"),
            Err(e) => match e {
                Error::TopicDoesNotExist(s) => assert_eq!(s, name),
                _ => panic!("Expected TopicDoesNotExist but received {:?}", e),
            },
        }
    }

    #[tokio::test]
    async fn publish_item() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name, None).await.unwrap();

        bus.publish::<Testing>(
            "testing",
            Testing {
                name: "blah".into(),
            },
        )
        .await
        .unwrap();

        let r: Vec<Item<Testing>> = bus.db.select(&name).await.unwrap();
        assert_eq!(r.len(), 1)
    }

    #[tokio::test]
    async fn try_publish_item() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name, Some(1)).await.unwrap();
        bus.try_publish::<Testing>(
            &name,
            Testing {
                name: "blah".into(),
            },
        )
        .await
        .unwrap();

        match bus
            .try_publish::<Testing>(
                &name,
                Testing {
                    name: "blah".into(),
                },
            )
            .await
        {
            Ok(_) => panic!("Expected Full error received None"),
            Err(e) => match e {
                TryPublishError::Full(_) => {}
                _ => panic!("Expected Full error received {:?}", e),
            },
        };
    }

    #[tokio::test]
    async fn publish_before_subscribe() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name, Some(1)).await.unwrap();

        bus.try_publish::<Testing>(
            &name,
            Testing {
                name: "blah".into(),
            },
        )
        .await
        .unwrap();

        match bus
            .try_publish::<Testing>(
                &name,
                Testing {
                    name: "blah".into(),
                },
            )
            .await
        {
            Ok(_) => panic!("Expected Full error received None"),
            Err(e) => match e {
                TryPublishError::Full(_) => {}
                _ => panic!("Expected Full error received {:?}", e),
            },
        };

        let mut topic = bus.subscribe::<Testing>(&name).await.unwrap().1;
        let _item1 = topic.recv().await.unwrap();

        drop(topic);
    }

    #[tokio::test]
    async fn publish_item_topic_does_not_exist() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        match bus
            .publish::<Testing>(
                &name,
                Testing {
                    name: "blah".into(),
                },
            )
            .await
        {
            Ok(_) => panic!("Expected error received none"),
            Err(e) => match e {
                Error::TopicDoesNotExist(s) => assert_eq!(s, name),
                _ => panic!("Expected TopicDoesNotExist received {:?}", e),
            },
        }
    }

    #[tokio::test]
    async fn subscribe_unsubscribe_success() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name, None).await.unwrap();
        bus.publish::<Testing>(
            &name,
            Testing {
                name: "blah".into(),
            },
        )
        .await
        .unwrap();

        let (sub_id, topic) = bus.subscribe::<Testing>(&name).await.unwrap();
        let r: Vec<Relationship> = bus.db.select("subscribe").await.unwrap();
        assert_eq!(r.len(), 1);

        bus.unsubscribe(&sub_id).await.unwrap();
        drop(topic);

        // up to 100 ms between sleeps, so sleeping for 125 ms to ensure item can be closed
        sleep(Duration::from_millis(125)).await;

        let r: Vec<Relationship> = bus.db.select("subscribe").await.unwrap();
        assert_eq!(r.len(), 0);
    }

    #[tokio::test]
    async fn subscription_created_topic_does_not_exist() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        match bus.subscribe::<Testing>(&name).await {
            Ok(_) => panic!("Expected error but received none"),
            Err(e) => match e {
                Error::TopicDoesNotExist(s) => assert_eq!(s, name),
                _ => panic!("expected TopicDoesNotExist received {:?}", e),
            },
        }
    }

    #[tokio::test]
    async fn subscriber_left_mid_stream() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name, None).await.unwrap();
        let mut subscriber1 = bus.subscribe::<Testing>(&name).await.unwrap().1;
        let mut subscriber2 = bus.subscribe::<Testing>(&name).await.unwrap().1;

        let ct: Vec<Topic> = bus.db.select("topics").await.unwrap();
        assert_eq!(ct.len(), 1);
        let r: Vec<Relationship> = bus.db.select("subscribe").await.unwrap();
        assert_eq!(r.len(), 2);

        bus.publish::<Testing>(
            &name,
            Testing {
                name: "blah1".into(),
            },
        )
        .await
        .unwrap();

        bus.publish::<Testing>(
            &name,
            Testing {
                name: "blah2".into(),
            },
        )
        .await
        .unwrap();

        bus.publish::<Testing>(
            &name,
            Testing {
                name: "blah3".into(),
            },
        )
        .await
        .unwrap();

        // receive item 1
        let item1a = subscriber1.recv().await.unwrap();
        assert_eq!(item1a.name, "blah1".to_string());

        let item1b = subscriber2.recv().await.unwrap();
        assert_eq!(item1b.name, "blah1".to_string());

        // receive item 2
        let item2a = subscriber1.recv().await.unwrap();
        assert_eq!(item2a.name, "blah2".to_string());

        // receive item 3
        let item3a = subscriber1.recv().await.unwrap();
        assert_eq!(item3a.name, "blah3".to_string());

        // end sub1
        drop(subscriber1);

        // end sub2
        drop(subscriber2);

        // up to 100 ms between sleeps, so sleeping for 125 ms to ensure item can be closed
        sleep(Duration::from_millis(125)).await;

        // check to ensure tables are empty
        let ct: Vec<Topic> = bus.db.select("topics").await.unwrap();
        assert_eq!(ct.len(), 1);

        let it: Vec<Item<Testing>> = bus.db.select(&name).await.unwrap();
        assert_eq!(it.len(), 0);

        bus.delete_topic(&name).await.unwrap();
        let ct: Vec<Topic> = bus.db.select("topics").await.unwrap();
        assert_eq!(ct.len(), 0);
    }

    #[tokio::test]
    async fn event_lifecycle() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name, None).await.unwrap();

        bus.publish::<Testing>(
            &name,
            Testing {
                name: "blah".into(),
            },
        )
        .await
        .unwrap();

        let exists: Vec<Item<Testing>> = bus.db.select(&name).await.unwrap();
        assert_eq!(exists.len(), 1);

        let (sub_id, mut topic) = bus.subscribe::<Testing>(&name).await.unwrap();
        let r: Vec<Relationship> = bus.db.select("subscribe").await.unwrap();
        assert_eq!(r.len(), 1);

        let _item = topic.try_recv().unwrap();

        // event received from all parties, should not longer be in table.
        let exists: Vec<Item<Testing>> = bus.db.select(&name).await.unwrap();
        assert_eq!(exists.len(), 0);
    }

    #[tokio::test]
    async fn it_works() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name, None).await.unwrap();
        let mut topic = bus.subscribe::<Testing>(&name).await.unwrap().1;
        bus.publish::<Testing>(
            &name,
            Testing {
                name: "blah".into(),
            },
        )
        .await
        .unwrap();
        let item = topic.recv().await.unwrap();
        assert_eq!(item.name, "blah".to_string());
    }
}
