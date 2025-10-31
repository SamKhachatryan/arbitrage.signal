#[macro_export]
macro_rules! subscribe_list {
    ($state:expr, $server:expr, $pair:expr, [ $( $svc:ident ),* ]) => {
        {
            use futures::future::{join_all, BoxFuture};

            let futs: Vec<BoxFuture<'static, ()>> = vec![
                $(
                    {
                        let cloned = Arc::clone(&$state);
                        let cloned_server = Arc::clone(&$server);
                        $svc::subscribe(cloned, cloned_server, $pair.to_string()).boxed()
                    },
                )*
            ];
            join_all(futs).await;
        }
    };
}