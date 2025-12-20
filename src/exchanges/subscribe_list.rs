#[macro_export]
macro_rules! subscribe_list {
    ($state:expr, $server:expr, $pair:expr, $cancel_token:expr, [ $( ( $session:expr, $url:expr ) ),* $(,)? ]) => {{
        use std::sync::Arc;
        use futures::future::{join_all, BoxFuture, FutureExt};

        let futs: Vec<BoxFuture<'static, ()>> = vec![
            $(
                {
                    // build client per (session, url)
                    let cloned_state = Arc::clone(&$state);
                    let cloned_server = Arc::clone(&$server);
                    let cloned_pair = $pair.clone();
                    let cloned_cancel = $cancel_token.clone();

                    async move {
                        let client = WSClient::new($session, $url);
                        client.subscribe(cloned_state, cloned_server, cloned_pair, cloned_cancel).await;
                    }
                    .boxed()
                }
            ),*
        ];

        join_all(futs).await;
    }};
}
