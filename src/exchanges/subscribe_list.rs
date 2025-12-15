#[macro_export]
macro_rules! subscribe_list {
    ($state:expr, $server:expr, $pair:expr, [ $( ( $session:expr, $url:expr ) ),* $(,)? ]) => {{
        use std::sync::Arc;
        use futures::future::{join_all, BoxFuture, FutureExt};

        let futs: Vec<BoxFuture<'static, ()>> = vec![
            $(
                {
                    // build client per (session, url)
                    let cloned_state = Arc::clone(&$state);
                    let cloned_server = Arc::clone(&$server);

                    async move {
                        let client = WSClient::new($session, $url);
                        client.subscribe(cloned_state, cloned_server, $pair).await;
                    }
                    .boxed()
                }
            ),*
        ];

        join_all(futs).await;
    }};
}
