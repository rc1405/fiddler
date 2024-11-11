use serde::Deserialize;
use serde::Serialize;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::engine::local::Mem;
use serde::de::DeserializeOwned;
use tokio::runtime::Handle;
use tokio::time::{sleep, Duration};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::broadcast;
use futures::StreamExt;
use uuid::Uuid;
use thiserror::Error;

pub type Subscription<T> = mpsc::Receiver<T>;

#[derive(Serialize, Deserialize, Debug)]
struct Topic {
    name: String,
    subscribers: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Item<T> {
    item_id: String,
    item: T,
    subscribers: Vec<String>,
    received_by: Vec<String>,
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
}



struct Connected {}
struct Disconnnected {}

#[allow(private_interfaces)]
pub struct EventBus<State = Disconnnected> {
    db: Surreal<Db>,
    state: std::marker::PhantomData<State>,
    runtime: tokio::runtime::Handle,
}

impl EventBus<Disconnnected> {
    #[allow(private_interfaces)]
    pub async fn new() -> Result<EventBus<Connected>, Error> {
        let db = Surreal::new::<Mem>(()).await.map_err(|_e| Error::UnableToInitializeEventBus)?;
        db.use_ns("eventbus").use_db("eventbus").await.map_err(|_e| Error::UnableToInitializeEventBus)?;

        Ok(EventBus{
            db,
            state: std::marker::PhantomData,
            runtime: Handle::current(),
        })
    }
}

impl EventBus<Connected> {
    pub async fn create_topic(&self, topic: &String) -> Result<(), Error> {
        let _: Option<Topic> = self.db.create(("topics", topic)).content(Topic {
            name: topic.clone(),
            subscribers: Vec::new(),
        }).await.map_err(|e| {
            if let surrealdb::Error::Db(e) = e {
                match e {
                    surrealdb::error::Db::RecordExists { thing: _ } => Error::TopicExists(topic.clone()),
                    _ => Error::InternalLookupError,
                }
            } else {
                Error::InternalLookupError
            }
            
        })?;
        Ok(())
    }

    pub async fn delete_topic(&self, topic: &String) -> Result<(), Error> {
        let t: Option<Topic> = self.db.delete(("topics", topic)).await.map_err(|_e| Error::InternalLookupError)?;
        if let None = t {
            return Err(Error::TopicDoesNotExist(topic.clone()))
        }
        let _v: Vec<serde_json::Value> = self.db.delete(topic).await.map_err(|_e| Error::TopicDeleteError)?;
        Ok(())
    }

    pub async fn publish<T: Serialize + DeserializeOwned + 'static>(&self, topic: &String, item: T) -> Result<(), Error> {
        let ct: Option<Topic> = self.db.select(("topics", topic)).await.map_err(|_e| Error::InternalLookupError)?;
        let t = match ct {
            Some(t) => t,
            None => return Err(Error::TopicDoesNotExist(topic.clone())),
        };

        let id: String = Uuid::new_v4().into();

        let i = Item{
            item_id: id.clone(),
            item,
            subscribers: t.subscribers.clone(),
            received_by: Vec::new(),
        };

        let _: Option<Item<T>> = self.db.create((topic, id)).content(i).await.map_err(|_e| Error::InternalLookupError)?;
        Ok(())
    }

    pub async fn subscribe<T: Send + Serialize + DeserializeOwned + std::marker::Unpin + 'static>(&self, topic: &String) -> Result<Subscription<T>, Error> {
        let exists = self.db.select::<Option<Topic>>(("topics", topic)).await.map_err(|_e| Error::InternalLookupError)?.is_some();
        if !exists {
            return Err(Error::TopicDoesNotExist(topic.clone()))
        };

        let subscriber_id: String = Uuid::new_v4().into();

        self.db
            .query(format!("UPDATE topics:`{}` SET subscribers += '{}';", topic, subscriber_id))
            .await.unwrap();

        let mut stream = self.db.select(topic).live().await.map_err(|_e| Error::InternalLookupError)?;
        let (sender, receiver) = mpsc::channel(1);
        let db = self.db.clone();
        let t = topic.clone();

        let (close_sender, mut close_receiver) = broadcast::channel(1);
        let cloned_sender = sender.clone();

        // this handler just watches for the receiving channel to be closed and initiate shutdown of the following thread
        self.runtime.spawn(async move {
            while !cloned_sender.is_closed() {
                sleep(Duration::from_millis(100)).await
            };

            let _ = close_sender.send(());
        });

        // launch thread to watch for live changes, aka publishes for this topic
        self.runtime.spawn(async move {
            let terminating_closure = async {
                db
                    // remove from subscribers
                    .query(format!("UPDATE topics:`{}` SET subscribers -= '{}';", t, subscriber_id))
                    // update any pending events for subscriber
                    .query(format!("UPDATE {0} SET received_by += '{1}' WHERE array::len(subscribers.filter(|$v| $v == '{1}')) > 0 AND array::len(received_by.filter(|$v| $v == '{1}')) == 0", t, subscriber_id))
                    // delete any events pending only this subscriber
                    .query(format!("DELETE {} WHERE array::boolean_and(array::sort(subscribers), array::sort(received_by));", t))
                    .await.unwrap();
            };

            
            loop {
                tokio::select! {
                    _ = close_receiver.recv() => {
                        terminating_closure.await;
                        return
                    },
                    s = stream.next() => {
                        if let Some(item) = s {
                            if let Ok(v) = item {
                                let i: Item<T> = v.data;
                                
                                // events are sent for every update to entries, including acknowledgements
                                //  we are ignoring any messages that have been acknowledged and only looking for new entries.
                                if i.received_by.len() == 0 {
                                    match db
                                        // acknowledge message received by subscriber
                                        .query(format!("UPDATE {}:`{}` SET received_by += '{}';", t, i.item_id, subscriber_id))
                                        // delete message if received by all subscribers when published
                                        .query(format!("DELETE {}:`{}` WHERE array::boolean_and(array::sort(subscribers), array::sort(received_by));", t, i.item_id))
                                        .await {
                                            Ok(_i) => {},
                                            Err(_e) => {
                                                terminating_closure.await;
                                                return
                                            },
                                    }
                                
                                    let mut data = i.item;
            
                                    while let Err(e) = sender.try_send(data) {
                                        match e {
                                            TrySendError::Full(d) => {
                                                data = d;
                                                sleep(Duration::from_millis(50)).await;
                                            },
                                            TrySendError::Closed(_) => {
                                                terminating_closure.await;
                                                return
                                            }
                                        }
                                    }
                                }
                                
                            } else {
                                terminating_closure.await;
                                return
                            }
                        } else {
                            terminating_closure.await;
                            return
                        }
                    }
                }
            }
        });        

        Ok(receiver)
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Clone, Debug)]
    struct Testing {
        name: String,
    }

    #[tokio::test]
    async fn create_bus() {
        let _bus = EventBus::new().await;
    }

    #[tokio::test]
    async fn create_topic() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name).await.unwrap();

        let ct: Vec<Topic> = bus.db.select("topics").await.unwrap();
        assert_eq!(ct.len(), 1)
    }

    #[tokio::test]
    async fn create_duplicate_topic() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name).await.unwrap();
        match bus.create_topic(&name).await {
            Ok(_) => panic!("Should have failed"),
            Err(e) => {
                match e {
                    Error::TopicExists(s) => assert_eq!(s, name),
                    _ => panic!("Expected TopicExists error received {:?}", e)
                }
            }
        }
    }

    #[tokio::test]
    async fn delete_topic() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name).await.unwrap();
        bus.delete_topic(&name).await.unwrap();
    }

    #[tokio::test]
    async fn delete_topic_does_not_exist() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        match bus.delete_topic(&name).await {
            Ok(_) => panic!("Expected error but received none"),
            Err(e) => {
                match e {
                    Error::TopicDoesNotExist(s) => assert_eq!(s, name),
                    _ => panic!("Expected TopicDoesNotExist but received {:?}", e),
                }
            }
        }
    }

    #[tokio::test]
    async fn publish_item() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name).await.unwrap();
        bus.publish::<Testing>(&name, Testing{ name: "blah".into()}).await.unwrap();
    }

    #[tokio::test]
    async fn publish_item_topic_does_not_exist() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        match bus.publish::<Testing>(&name, Testing{ name: "blah".into()}).await {
            Ok(_) => panic!("Expected error received none"),
            Err(e) => {
                match e {
                    Error::TopicDoesNotExist(s) => assert_eq!(s, name),
                    _ => panic!("Expected TopicDoesNotExist received {:?}", e),
                }
            },
        }
    }

    #[tokio::test]
    async fn subscription_created() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name).await.unwrap();
        
        let topic = bus.subscribe::<Testing>(&name).await.unwrap();
        let ct: Option<Topic> = bus.db.select(("topics", &name)).await.unwrap();
        let subscription_count = match ct {
            Some(t) => t.subscribers.len(),
            None => panic!("topic not found"),
        };
        assert_eq!(subscription_count, 1);
        drop(topic);
    }

    #[tokio::test]
    async fn subscription_ended() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name).await.unwrap();
        bus.publish::<Testing>(&name, Testing{ name: "blah".into()}).await.unwrap();

        let topic = bus.subscribe::<Testing>(&name).await.unwrap();        
        drop(topic);

        // up to 100 ms between sleeps, so sleeping for 125 ms to ensure item can be closed
        sleep(Duration::from_millis(125)).await;
        let ct: Option<Topic> = bus.db.select(("topics", &name)).await.unwrap();
        let subscription_count = match ct {
            Some(t) => t.subscribers,
            None => panic!("topic not found"),
        };
        assert_eq!(subscription_count.len(), 0);
    }

    #[tokio::test]
    async fn subscription_created_topic_does_not_exist() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        match bus.subscribe::<Testing>(&name).await {
            Ok(_) => panic!("Expected error but received none"),
            Err(e) => {
                match e {
                    Error::TopicDoesNotExist(s) => assert_eq!(s, name),
                    _ => panic!("expected TopicDoesNotExist received {:?}", e),
                }
            }
        }
    }

    #[tokio::test]
    async fn subscriber_left_mid_stream() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name).await.unwrap();
        let mut subscriber1 = bus.subscribe::<Testing>(&name).await.unwrap();
        let mut subscriber2 = bus.subscribe::<Testing>(&name).await.unwrap();

        let ct: Vec<Topic> = bus.db.select("topics").await.unwrap();
        assert_eq!(ct.len(), 1);
        assert_eq!(ct[0].subscribers.len(), 2);

        bus.publish::<Testing>(&name, Testing{ name: "blah1".into()}).await.unwrap();
        bus.publish::<Testing>(&name, Testing{ name: "blah2".into()}).await.unwrap();
        bus.publish::<Testing>(&name, Testing{ name: "blah3".into()}).await.unwrap();
       
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
    async fn it_works() {
        let name = String::from("testing");
        let bus = EventBus::new().await.unwrap();
        bus.create_topic(&name).await.unwrap();
        let mut topic = bus.subscribe::<Testing>(&name).await.unwrap();
        bus.publish::<Testing>(&name, Testing{ name: "blah".into()}).await.unwrap();
        let item = topic.recv().await.unwrap();
        assert_eq!(item.name, "blah".to_string());
    }
}
